//! The session's connections: the host holds one to each guest and passes on what each says; a
//! guest holds one, to the host. One thread reads and one writes per connection, and at the host a
//! hub thread owns the list of guests. The window talks to them through channels.

use crate::i18n::tx;
use crate::tr;
use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use super::wire::{self, Cipher, Key, Message};

const MAX_GUESTS: usize = 16;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// Largest file sent to guests, and the most sent in all.
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_FILES_TOTAL: u64 = 256 * 1024 * 1024;

/// What the network tells the window.
#[derive(Debug)]
pub enum Incoming {
    /// Guest: the connection is up.
    Connected,
    /// Host: someone came in, by their connection number.
    Joined {
        peer: u64,
        name: String,
    },
    /// Host: someone left.
    Left {
        name: String,
        cursor: Option<u64>,
    },
    Message(Message),
    /// Guest: a file the document uses arrived.
    FileArrived,
    /// The session is over for this machine.
    Ended(String),
}

/// What the window tells the network.
pub enum Outgoing {
    /// To everyone.
    Broadcast(Message),
    /// Host: to one guest.
    To(u64, Message),
    /// Host: the files the document now uses (path from its folder, path on disk).
    Files(Vec<(String, PathBuf)>),
    Stop,
}

/// Wakes the window when something arrives.
pub type Waker = Arc<OnceLock<egui::Context>>;

fn wake(waker: &Waker) {
    if let Some(ctx) = waker.get() {
        ctx.request_repaint();
    }
}

enum HubEvent {
    Window(Outgoing),
    Accepted { peer: u64, stream: TcpStream, writer: Sender<Arc<Vec<u8>>> },
    Read { peer: u64, message: Message },
    Closed { peer: u64 },
}

/// The window's end of the host's or guest's channel.
pub struct Link {
    send: Box<dyn Fn(Outgoing) + Send>,
    pub incoming: Receiver<Incoming>,
}

impl Link {
    pub fn send(&self, out: Outgoing) {
        (self.send)(out);
    }
}

fn writer_loop(mut stream: TcpStream, cipher: Cipher, messages: Receiver<Arc<Vec<u8>>>) {
    while let Ok(message) = messages.recv() {
        if wire::send(&mut stream, &cipher, &message).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

/// The address machines on this network reach this one at. Connecting a UDP socket only picks a
/// route; nothing is sent.
fn local_address() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map_or_else(|_| "127.0.0.1".to_string(), |a| a.ip().to_string())
}

/// Starts hosting on `bind` (port 0 for any free one); returns the channel and the address to share.
pub fn host(bind: &str, key: Key, waker: Waker) -> io::Result<(Link, String)> {
    let listener = TcpListener::bind(bind)?;
    let port = listener.local_addr()?.port();
    let loopback = listener.local_addr()?.ip().is_loopback();
    let address = format!("{}:{port}", if loopback { "127.0.0.1".to_string() } else { local_address() });
    listener.set_nonblocking(true)?;
    let (to_window, incoming) = channel();
    let (hub, events) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let guests = Arc::new(AtomicUsize::new(0));
    {
        let (hub, stop, guests) = (hub.clone(), Arc::clone(&stop), Arc::clone(&guests));
        std::thread::Builder::new().name("pearl-accept".into()).spawn(move || accept_loop(&listener, key, &hub, &stop, &guests))?;
    }
    {
        let waker = waker.clone();
        std::thread::Builder::new().name("pearl-hub".into()).spawn(move || Hub::new(to_window, waker, guests).run(&events))?;
    }
    let window_hub = hub;
    let send = Box::new(move |out: Outgoing| {
        if matches!(out, Outgoing::Stop) {
            stop.store(true, Ordering::Relaxed);
        }
        let _ = window_hub.send(HubEvent::Window(out));
    });
    Ok((Link { send, incoming }, address))
}

fn accept_loop(listener: &TcpListener, key: Key, hub: &Sender<HubEvent>, stop: &AtomicBool, guests: &Arc<AtomicUsize>) {
    let mut next_peer = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                // Refused before the handshake's work: a full session, or a stranger knocking.
                if guests.load(Ordering::Relaxed) >= MAX_GUESTS {
                    continue;
                }
                next_peer += 1;
                let (peer, hub) = (next_peer, hub.clone());
                let _ = std::thread::Builder::new().name("pearl-peer".into()).spawn(move || serve_guest(stream, key, peer, &hub));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn serve_guest(mut stream: TcpStream, key: Key, peer: u64, hub: &Sender<HubEvent>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let Ok(cipher) = wire::handshake(&mut stream, &key, false) else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let (writer, messages) = channel();
    let (Ok(for_writer), Ok(for_hub)) = (stream.try_clone(), stream.try_clone()) else { return };
    let cipher_w = Arc::clone(&cipher);
    if std::thread::Builder::new().name("pearl-write".into()).spawn(move || writer_loop(for_writer, cipher_w, messages)).is_err() {
        return;
    }
    if hub.send(HubEvent::Accepted { peer, stream: for_hub, writer }).is_err() {
        return;
    }
    while let Ok(bytes) = wire::receive(&mut stream, &cipher) {
        // A message that can't be read means the other side is broken: drop it.
        let Some(message) = Message::decode(&bytes) else { break };
        if hub.send(HubEvent::Read { peer, message }).is_err() {
            break;
        }
    }
    let _ = hub.send(HubEvent::Closed { peer });
}

/// When a file last changed and how long it is: what tells that it must be sent again.
type FileStamp = (Option<SystemTime>, u64);

struct Guest {
    stream: TcpStream,
    writer: Sender<Arc<Vec<u8>>>,
    /// Empty until it has said hello; only then does it hear or say anything.
    name: Option<String>,
    cursor: Option<u64>,
}

struct Hub {
    to_window: Sender<Incoming>,
    waker: Waker,
    count: Arc<AtomicUsize>,
    guests: HashMap<u64, Guest>,
    files: Vec<(String, PathBuf)>,
    sent: HashMap<String, FileStamp>,
}

impl Hub {
    fn new(to_window: Sender<Incoming>, waker: Waker, count: Arc<AtomicUsize>) -> Hub {
        Hub { to_window, waker, count, guests: HashMap::new(), files: Vec::new(), sent: HashMap::new() }
    }

    fn tell_window(&self, incoming: Incoming) {
        let _ = self.to_window.send(incoming);
        wake(&self.waker);
    }

    fn broadcast(&self, message: &Message, except: Option<u64>) {
        let encoded = Arc::new(message.encode());
        for (peer, guest) in &self.guests {
            if Some(*peer) != except && guest.name.is_some() {
                let _ = guest.writer.send(Arc::clone(&encoded));
            }
        }
    }

    fn run(mut self, events: &Receiver<HubEvent>) {
        while let Ok(event) = events.recv() {
            match event {
                HubEvent::Window(Outgoing::Stop) => break,
                HubEvent::Window(Outgoing::Broadcast(message)) => self.broadcast(&message, None),
                HubEvent::Window(Outgoing::To(peer, message)) => {
                    if let Some(guest) = self.guests.get(&peer) {
                        let _ = guest.writer.send(Arc::new(message.encode()));
                    }
                }
                HubEvent::Window(Outgoing::Files(files)) => {
                    self.files = files;
                    self.send_changed_files();
                }
                HubEvent::Accepted { peer, stream, writer } => {
                    self.guests.insert(peer, Guest { stream, writer, name: None, cursor: None });
                    self.count.store(self.guests.len(), Ordering::Relaxed);
                }
                HubEvent::Read { peer, message } => self.read(peer, message),
                HubEvent::Closed { peer } => self.closed(peer),
            }
        }
        for guest in self.guests.values() {
            let _ = guest.stream.shutdown(Shutdown::Both);
        }
    }

    fn read(&mut self, peer: u64, message: Message) {
        match message {
            Message::Hello { name } => {
                let Some(guest) = self.guests.get_mut(&peer) else { return };
                if guest.name.is_some() {
                    return;
                }
                guest.name = Some(name.clone());
                self.tell_window(Incoming::Joined { peer, name });
                self.broadcast(&Message::AskCursors, Some(peer));
                self.send_all_files_to(peer);
            }
            Message::Update(_) | Message::Cursor(_) => {
                if !self.guests.get(&peer).is_some_and(|g| g.name.is_some()) {
                    return;
                }
                if let (Message::Cursor(c), Some(guest)) = (&message, self.guests.get_mut(&peer)) {
                    guest.cursor = Some(c.id);
                }
                self.broadcast(&message, Some(peer));
                self.tell_window(Incoming::Message(message));
            }
            // A guest has no business sending the rest.
            _ => {}
        }
    }

    fn closed(&mut self, peer: u64) {
        let Some(guest) = self.guests.remove(&peer) else { return };
        self.count.store(self.guests.len(), Ordering::Relaxed);
        if let Some(name) = guest.name {
            if let Some(id) = guest.cursor {
                self.broadcast(&Message::Leave { id }, None);
            }
            self.tell_window(Incoming::Left { name, cursor: guest.cursor });
        }
    }

    fn read_file(path: &Path) -> Option<(FileStamp, Vec<u8>)> {
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() || meta.len() > MAX_FILE {
            return None;
        }
        Some(((meta.modified().ok(), meta.len()), std::fs::read(path).ok()?))
    }

    fn send_changed_files(&mut self) {
        let mut total = 0;
        for (relative, path) in self.files.clone() {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            total += meta.len();
            let stamp = (meta.modified().ok(), meta.len());
            if total > MAX_FILES_TOTAL || self.sent.get(&relative) == Some(&stamp) {
                continue;
            }
            if let Some((stamp, bytes)) = Self::read_file(&path) {
                self.sent.insert(relative.clone(), stamp);
                self.broadcast(&Message::File { path: relative, bytes }, None);
            }
        }
    }

    fn send_all_files_to(&self, peer: u64) {
        let Some(guest) = self.guests.get(&peer) else { return };
        let mut total = 0;
        for (relative, path) in &self.files {
            if let Some((stamp, bytes)) = Self::read_file(path) {
                total += stamp.1;
                if total > MAX_FILES_TOTAL {
                    break;
                }
                let _ = guest.writer.send(Arc::new(Message::File { path: relative.clone(), bytes }.encode()));
            }
        }
    }
}

/// Connects to a host, in the background; received files go under `folder`.
pub fn join(address: String, key: Key, name: String, folder: PathBuf, waker: Waker) -> io::Result<Link> {
    let (to_window, incoming) = channel();
    let (writer, messages) = channel::<Arc<Vec<u8>>>();
    let stream_slot: Arc<std::sync::Mutex<Option<TcpStream>>> = Arc::default();
    let slot = Arc::clone(&stream_slot);
    std::thread::Builder::new().name("pearl-guest".into()).spawn(move || {
        let ended = guest_loop(&address, &key, &name, &folder, &to_window, messages, &waker, &slot);
        let _ = to_window.send(Incoming::Ended(ended));
        wake(&waker);
    })?;
    let send = Box::new(move |out: Outgoing| match out {
        Outgoing::Broadcast(message) | Outgoing::To(_, message) => {
            let _ = writer.send(Arc::new(message.encode()));
        }
        Outgoing::Files(_) => {}
        Outgoing::Stop => {
            if let Ok(slot) = stream_slot.lock()
                && let Some(stream) = slot.as_ref()
            {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    });
    Ok(Link { send, incoming })
}

#[allow(clippy::too_many_arguments)]
fn guest_loop(
    address: &str,
    key: &Key,
    name: &str,
    folder: &Path,
    to_window: &Sender<Incoming>,
    messages: Receiver<Arc<Vec<u8>>>,
    waker: &Waker,
    slot: &std::sync::Mutex<Option<TcpStream>>,
) -> String {
    let unreachable = || tr!("cant-connect", address = address.to_string());
    let Some(target) = address.to_socket_addrs().ok().and_then(|mut a| a.next()) else { return unreachable() };
    let Ok(mut stream) = TcpStream::connect_timeout(&target, CONNECT_TIMEOUT) else { return unreachable() };
    let _ = stream.set_nodelay(true);
    let Ok(cipher) = wire::handshake(&mut stream, key, true) else {
        return tx("the-session-refused-the-connection-check-that").to_string();
    };
    if let (Ok(mut slot), Ok(clone)) = (slot.lock(), stream.try_clone()) {
        *slot = Some(clone);
    }
    let Ok(for_writer) = stream.try_clone() else { return unreachable() };
    let cipher_w = Arc::clone(&cipher);
    if std::thread::Builder::new().name("pearl-write".into()).spawn(move || writer_loop(for_writer, cipher_w, messages)).is_err() {
        return unreachable();
    }
    if wire::send(&mut stream, &cipher, &Message::Hello { name: name.to_string() }.encode()).is_err() {
        return unreachable();
    }
    let _ = to_window.send(Incoming::Connected);
    wake(waker);
    loop {
        let Ok(bytes) = wire::receive(&mut stream, &cipher) else {
            return tx("the-session-ended").to_string();
        };
        let Some(message) = Message::decode(&bytes) else {
            return tx("the-session-sent-invalid-data-and-was").to_string();
        };
        let incoming = match message {
            // Written here, off the window's thread, and only inside the session's folder.
            Message::File { path, bytes } => {
                let Some(target) = wire::safe_path(folder, &path) else { continue };
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(target, bytes).is_err() {
                    continue;
                }
                Incoming::FileArrived
            }
            other => Incoming::Message(other),
        };
        if to_window.send(incoming).is_err() {
            return String::new();
        }
        wake(waker);
    }
}
