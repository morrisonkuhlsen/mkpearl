//! Editing together in real time, with no server: one person opens a session and shares a link; the
//! others type into the same document and see each other's carets. The host's machine holds the
//! file; the session ends when the host leaves. Only the main document is edited together; the
//! pictures and other files it uses travel to the guests, who get them read-only.

mod doc;
mod net;
pub mod wire;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use yrs::StickyIndex;

pub use self::doc::{SharedText, diff};
use self::net::{Incoming, Link, Outgoing};
use self::wire::{CursorInfo, Message};

/// What a poll found, for the window to act on.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    /// The text changed; it's in [`Session::text`].
    Text,
    /// A file the document uses arrived: compile again.
    File,
    Joined(String),
    Left(String),
    /// The session is over, for this reason.
    Ended(String),
}

/// Someone else's caret, for drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCursor {
    pub id: u64,
    pub name: String,
    pub byte: usize,
}

struct Remote {
    name: String,
    head: StickyIndex,
}

pub struct Session {
    link: Link,
    shared: SharedText,
    id: u64,
    name: String,
    /// The link to give others (host only).
    share_link: Option<String>,
    /// The link this session was joined by, or that it's hosted at.
    link_text: String,
    is_host: bool,
    /// Guest: still waiting for the document.
    waiting: bool,
    file_name: String,
    /// Guest: where the host's files are kept, for the session's length.
    folder: Option<PathBuf>,
    remotes: HashMap<u64, Remote>,
    /// Host: the guests here, by name.
    people: Vec<String>,
    last_cursor: Option<(usize, usize)>,
    /// A guest arrived: say where the caret is again.
    resend_cursor: bool,
    waker: net::Waker,
}

impl Session {
    /// Opens a session on `text`, listening on every network this machine is on.
    pub fn host(text: &str, name: &str, file_name: &str) -> Result<Session, String> {
        Session::host_on("0.0.0.0:0", text, name, file_name)
    }

    /// [`Session::host`] on a chosen address.
    pub fn host_on(bind: &str, text: &str, name: &str, file_name: &str) -> Result<Session, String> {
        let key = wire::new_key().map_err(|e| e.to_string())?;
        let waker: net::Waker = Arc::new(OnceLock::new());
        let (link, address) = net::host(bind, key, Arc::clone(&waker)).map_err(|e| format!("Não foi possível abrir a sessão: {e}"))?;
        Ok(Session {
            link,
            shared: SharedText::new(text)?,
            id: random_id()?,
            name: wire::clean_name(name),
            share_link: Some(wire::format_link(&address, &key)),
            link_text: wire::format_link(&address, &key),
            is_host: true,
            waiting: false,
            file_name: wire::safe_file_name(file_name),
            folder: None,
            remotes: HashMap::new(),
            people: Vec::new(),
            last_cursor: None,
            resend_cursor: false,
            waker,
        })
    }

    /// Joins the session behind `link`. The document arrives later, as [`Change::Text`].
    pub fn join(link: &str, name: &str) -> Result<Session, String> {
        let link_text = link.trim().to_string();
        let (address, key) = wire::parse_link(link).ok_or_else(|| "Esse não é um link de sessão do Pearl.".to_string())?;
        let id = random_id()?;
        let folder = std::env::temp_dir().join(format!("pearl-session-{id:016x}"));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let name = wire::clean_name(name);
        let waker: net::Waker = Arc::new(OnceLock::new());
        let link = net::join(address, key, name.clone(), folder.clone(), Arc::clone(&waker)).map_err(|e| e.to_string())?;
        Ok(Session {
            link,
            shared: SharedText::new("")?,
            id,
            name,
            share_link: None,
            link_text,
            is_host: false,
            waiting: true,
            file_name: "documento.typ".to_string(),
            folder: Some(folder),
            remotes: HashMap::new(),
            people: Vec::new(),
            last_cursor: None,
            resend_cursor: false,
            waker,
        })
    }

    /// Lets arriving messages wake the window.
    pub fn set_waker(&self, ctx: &egui::Context) {
        let _ = self.waker.set(ctx.clone());
    }

    pub fn text(&self) -> &str {
        self.shared.text()
    }

    pub fn is_host(&self) -> bool {
        self.is_host
    }

    /// Guest: the document hasn't arrived yet.
    pub fn is_waiting(&self) -> bool {
        self.waiting
    }

    pub fn share_link(&self) -> Option<&str> {
        self.share_link.as_deref()
    }

    /// The link that gets into this session, for Claude to come in by.
    pub fn link(&self) -> &str {
        &self.link_text
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Everyone else, by name: the guests at the host; the carets seen at a guest.
    pub fn others(&self) -> Vec<String> {
        if self.is_host {
            self.people.clone()
        } else {
            let mut names: Vec<String> = self.remotes.values().map(|r| r.name.clone()).collect();
            names.sort();
            names
        }
    }

    /// Guest: the path the compiler should treat as the document's, in the session's folder.
    pub fn guest_path(&self) -> Option<PathBuf> {
        self.folder.as_ref().map(|f| f.join(&self.file_name))
    }

    /// Takes in what the editor shows and sends any change.
    pub fn sync_local(&mut self, text: &str) {
        if !self.waiting {
            self.shared.apply_local(text);
        }
        self.flush();
    }

    fn flush(&mut self) {
        for update in self.shared.take_outgoing() {
            self.link.send(Outgoing::Broadcast(Message::Update(update)));
        }
    }

    /// Says where the caret is, when it moved or someone new needs to know.
    pub fn send_cursor(&mut self, anchor: usize, head: usize) {
        if self.waiting || (self.last_cursor == Some((anchor, head)) && !self.resend_cursor) {
            return;
        }
        let (Some(a), Some(h)) = (self.shared.pin(anchor), self.shared.pin(head)) else { return };
        self.last_cursor = Some((anchor, head));
        self.resend_cursor = false;
        let info = CursorInfo { id: self.id, name: self.name.clone(), anchor: SharedText::encode_pin(&a), head: SharedText::encode_pin(&h) };
        self.link.send(Outgoing::Broadcast(Message::Cursor(info)));
    }

    /// Ties the local selection to the text, to survive merging others' changes.
    pub fn pin(&self, byte: usize) -> Option<StickyIndex> {
        self.shared.pin(byte)
    }

    pub fn locate(&self, pin: &StickyIndex) -> Option<usize> {
        self.shared.locate(pin)
    }

    /// Everyone else's carets, where they are now in the text.
    pub fn cursors(&self) -> Vec<RemoteCursor> {
        let mut out: Vec<RemoteCursor> =
            self.remotes.iter().filter_map(|(id, r)| Some(RemoteCursor { id: *id, name: r.name.clone(), byte: self.shared.locate(&r.head)? })).collect();
        out.sort_by_key(|c| c.id);
        out
    }

    /// Undoes this person's last change only.
    pub fn undo(&mut self) -> bool {
        let done = self.shared.undo();
        self.flush();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.shared.redo();
        self.flush();
        done
    }

    /// Host: the files the document now uses go to the guests.
    pub fn share_files(&self, files: Vec<(String, PathBuf)>) {
        if self.is_host {
            self.link.send(Outgoing::Files(files));
        }
    }

    /// Takes in what arrived.
    pub fn poll(&mut self) -> Vec<Change> {
        let incoming: Vec<Incoming> = self.link.incoming.try_iter().collect();
        let mut changes = Vec::new();
        for item in incoming {
            match item {
                Incoming::Connected | Incoming::Message(Message::Hello { .. }) | Incoming::Message(Message::File { .. }) => {}
                Incoming::Joined { peer, name } => {
                    self.people.push(name.clone());
                    let welcome = Message::Welcome { file_name: self.file_name.clone(), state: self.shared.state() };
                    self.link.send(Outgoing::To(peer, welcome));
                    self.resend_cursor = true;
                    changes.push(Change::Joined(name));
                }
                Incoming::Left { name, cursor } => {
                    if let Some(place) = self.people.iter().position(|p| *p == name) {
                        self.people.remove(place);
                    }
                    if let Some(id) = cursor {
                        self.remotes.remove(&id);
                    }
                    changes.push(Change::Left(name));
                }
                Incoming::FileArrived => changes.push(Change::File),
                Incoming::Ended(reason) => changes.push(Change::Ended(reason)),
                Incoming::Message(Message::Welcome { file_name, state }) => {
                    if !self.is_host && self.waiting {
                        self.file_name = wire::safe_file_name(&file_name);
                        match self.shared.apply_remote(&state) {
                            Ok(_) => {
                                self.waiting = false;
                                self.resend_cursor = true;
                                changes.push(Change::Text);
                            }
                            Err(_) => changes.push(Change::Ended("A sessão enviou um documento inválido.".to_string())),
                        }
                    }
                }
                Incoming::Message(Message::Update(update)) => match self.shared.apply_remote(&update) {
                    Ok(true) => changes.push(Change::Text),
                    Ok(false) => {}
                    Err(_) if !self.is_host => changes.push(Change::Ended("A sessão enviou dados inválidos.".to_string())),
                    // At the host a guest's bad update is that guest's problem, not the session's.
                    Err(_) => {}
                },
                Incoming::Message(Message::Cursor(c)) => {
                    if let Some(head) = SharedText::decode_pin(&c.head) {
                        self.remotes.insert(c.id, Remote { name: c.name, head });
                    }
                }
                Incoming::Message(Message::AskCursors) => self.resend_cursor = true,
                Incoming::Message(Message::Leave { id }) => {
                    self.remotes.remove(&id);
                }
            }
        }
        changes
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.link.send(Outgoing::Stop);
        if let Some(folder) = &self.folder {
            let _ = std::fs::remove_dir_all(folder);
        }
    }
}

fn random_id() -> Result<u64, String> {
    getrandom::u64().map_err(|e| e.to_string())
}
