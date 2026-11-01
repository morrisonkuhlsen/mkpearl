//! What travels between the machines of a session: the link, the encrypted frames and the messages.
//!
//! The link carries a random 256-bit key. Both sides run the Noise `NNpsk0` handshake with it, so
//! without the link nobody can join or read the traffic, and a captured session can't be brute
//! forced from a guessable code. Everything read from the network is hostile: lengths are checked
//! and capped, and a bad message ends that connection, never the program.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use snow::{Builder, TransportState};

const NOISE: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";
const SCHEME: &str = "pearl://";
/// Largest plaintext in one Noise frame (a Noise message is at most 65535 bytes with its tag).
const CHUNK: usize = 60_000;
/// Largest message accepted: a picture or the whole document's state.
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest name shown over a cursor.
const MAX_NAME: usize = 40;
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub type Key = [u8; 32];

/// A new random key.
pub fn new_key() -> io::Result<Key> {
    let mut key = [0; 32];
    getrandom::fill(&mut key).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(key)
}

fn encode_key(key: &Key) -> String {
    let (mut out, mut buffer, mut bits) = (String::with_capacity(52), 0_u32, 0);
    for byte in key {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
    }
    out
}

fn decode_key(text: &str) -> Option<Key> {
    let mut key = [0; 32];
    let (mut written, mut buffer, mut bits) = (0, 0_u32, 0);
    for c in text.bytes() {
        let value = ALPHABET.iter().position(|a| *a == c.to_ascii_uppercase())? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            *key.get_mut(written)? = (buffer >> bits) as u8;
            written += 1;
            buffer &= (1 << bits) - 1;
        }
    }
    (written == key.len()).then_some(key)
}

/// `pearl://address:port#key`.
pub fn format_link(address: &str, key: &Key) -> String {
    format!("{SCHEME}{address}#{}", encode_key(key))
}

/// The address and key in a link, or `None` when it isn't one.
pub fn parse_link(link: &str) -> Option<(String, Key)> {
    let (address, key) = link.trim().strip_prefix(SCHEME)?.split_once('#')?;
    let (host, port) = address.rsplit_once(':')?;
    if host.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    Some((address.to_string(), decode_key(key)?))
}

/// The encrypted connection's state, used for one direction at a time under a lock.
pub type Cipher = std::sync::Arc<std::sync::Mutex<TransportState>>;

fn bad(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn noise(key: &Key) -> io::Result<Builder<'_>> {
    Builder::new(NOISE.parse().map_err(bad)?).psk(0, key).map_err(bad)
}

fn write_frame(stream: &mut TcpStream, frame: &[u8]) -> io::Result<()> {
    let len = u16::try_from(frame.len()).map_err(bad)?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(frame)
}

fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len = [0; 2];
    stream.read_exact(&mut len)?;
    let mut frame = vec![0; usize::from(u16::from_be_bytes(len))];
    stream.read_exact(&mut frame)?;
    Ok(frame)
}

/// Runs the handshake; `initiator` is the side that connected. Fails on a wrong key.
pub fn handshake(stream: &mut TcpStream, key: &Key, initiator: bool) -> io::Result<Cipher> {
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    let mut buffer = vec![0; 1024];
    let mut state = if initiator { noise(key)?.build_initiator() } else { noise(key)?.build_responder() }.map_err(bad)?;
    if initiator {
        let n = state.write_message(&[], &mut buffer).map_err(bad)?;
        write_frame(stream, buffer.get(..n).unwrap_or_default())?;
        state.read_message(&read_frame(stream)?, &mut buffer).map_err(bad)?;
    } else {
        state.read_message(&read_frame(stream)?, &mut buffer).map_err(bad)?;
        let n = state.write_message(&[], &mut buffer).map_err(bad)?;
        write_frame(stream, buffer.get(..n).unwrap_or_default())?;
    }
    stream.set_read_timeout(None)?;
    let transport = state.into_transport_mode().map_err(bad)?;
    Ok(std::sync::Arc::new(std::sync::Mutex::new(transport)))
}

/// Sends one message: its length, then its bytes, encrypted in frames.
pub fn send(stream: &mut TcpStream, cipher: &Cipher, message: &[u8]) -> io::Result<()> {
    let total = u32::try_from(message.len()).map_err(bad)?;
    let mut framed = Vec::with_capacity(message.len() + 4);
    framed.extend_from_slice(&total.to_be_bytes());
    framed.extend_from_slice(message);
    let mut out = vec![0; CHUNK + 64];
    for chunk in framed.chunks(CHUNK) {
        let n = cipher.lock().map_err(|_| bad("cipher"))?.write_message(chunk, &mut out).map_err(bad)?;
        write_frame(stream, out.get(..n).unwrap_or_default())?;
    }
    Ok(())
}

/// Receives one message, refusing one longer than [`MAX_MESSAGE`].
pub fn receive(stream: &mut TcpStream, cipher: &Cipher) -> io::Result<Vec<u8>> {
    let mut plain = vec![0; CHUNK + 64];
    let mut message = Vec::new();
    let mut remaining = None;
    loop {
        let frame = read_frame(stream)?;
        let n = cipher.lock().map_err(|_| bad("cipher"))?.read_message(&frame, &mut plain).map_err(bad)?;
        let mut part = plain.get(..n).unwrap_or_default();
        let left = match remaining {
            Some(left) => left,
            None => {
                let head: [u8; 4] = part.get(..4).and_then(|h| h.try_into().ok()).ok_or_else(|| bad("short message"))?;
                part = part.get(4..).unwrap_or_default();
                let total = u32::from_be_bytes(head) as usize;
                if total > MAX_MESSAGE {
                    return Err(bad("message too long"));
                }
                total
            }
        };
        if part.len() > left {
            return Err(bad("message longer than announced"));
        }
        message.extend_from_slice(part);
        let left = left - part.len();
        if left == 0 {
            return Ok(message);
        }
        remaining = Some(left);
    }
}

/// Where someone's caret is, as positions that stay put while the text changes around them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorInfo {
    pub id: u64,
    pub name: String,
    pub anchor: Vec<u8>,
    pub head: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// A guest introducing itself.
    Hello {
        name: String,
    },
    /// The host's answer: the document's name and its whole state.
    Welcome {
        file_name: String,
        state: Vec<u8>,
    },
    /// A change to the text.
    Update(Vec<u8>),
    Cursor(CursorInfo),
    /// Everyone should say where their caret is (someone new has come in).
    AskCursors,
    /// Someone left; their caret goes away.
    Leave {
        id: u64,
    },
    /// A file the document uses, by its path from the document's folder.
    File {
        path: String,
        bytes: Vec<u8>,
    },
}

fn put(out: &mut Vec<u8>, field: &[u8]) {
    out.extend_from_slice(&(field.len() as u32).to_be_bytes());
    out.extend_from_slice(field);
}

struct Fields<'a>(&'a [u8]);

impl<'a> Fields<'a> {
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        let n = u32::from_be_bytes(*head) as usize;
        let (field, rest) = (rest.get(..n)?, rest.get(n..)?);
        self.0 = rest;
        Some(field)
    }
    fn string(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?.to_vec()).ok()
    }
    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_be_bytes(*head))
    }
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Message::Hello { name } => {
                out.push(1);
                put(&mut out, name.as_bytes());
            }
            Message::Welcome { file_name, state } => {
                out.push(2);
                put(&mut out, file_name.as_bytes());
                put(&mut out, state);
            }
            Message::Update(update) => {
                out.push(3);
                put(&mut out, update);
            }
            Message::Cursor(c) => {
                out.push(4);
                out.extend_from_slice(&c.id.to_be_bytes());
                put(&mut out, c.name.as_bytes());
                put(&mut out, &c.anchor);
                put(&mut out, &c.head);
            }
            Message::AskCursors => out.push(5),
            Message::Leave { id } => {
                out.push(6);
                out.extend_from_slice(&id.to_be_bytes());
            }
            Message::File { path, bytes } => {
                out.push(7);
                put(&mut out, path.as_bytes());
                put(&mut out, bytes);
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Message> {
        let (tag, rest) = bytes.split_first()?;
        let mut f = Fields(rest);
        Some(match tag {
            1 => Message::Hello { name: clean_name(&f.string()?) },
            2 => Message::Welcome { file_name: f.string()?, state: f.bytes()?.to_vec() },
            3 => Message::Update(f.bytes()?.to_vec()),
            4 => Message::Cursor(CursorInfo { id: f.u64()?, name: clean_name(&f.string()?), anchor: f.bytes()?.to_vec(), head: f.bytes()?.to_vec() }),
            5 => Message::AskCursors,
            6 => Message::Leave { id: f.u64()? },
            7 => Message::File { path: f.string()?, bytes: f.bytes()?.to_vec() },
            _ => return None,
        })
    }
}

/// A name fit to show: one line, not too long.
pub fn clean_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    let clean = clean.trim();
    if clean.is_empty() { crate::i18n::tx("guest").to_string() } else { clean.to_string() }
}

/// Where a received file goes inside `root`, or `None` for a path that would leave it.
pub fn safe_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    let mut any = false;
    for part in relative.split('/') {
        let dangerous = part.is_empty() || part == "." || part == ".." || part.contains(['\\', ':', '\0']);
        if dangerous {
            return None;
        }
        out.push(part);
        any = true;
    }
    any.then_some(out)
}

/// The host's document name, reduced to a plain `.typ` file name.
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let usable = base.ends_with(".typ") && base.len() > 4 && !base.contains([':', '\0']) && base != "..typ";
    if usable { base.to_string() } else { "documento.typ".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_round_trips() {
        let key = new_key().unwrap();
        let link = format_link("192.168.0.7:4242", &key);
        assert_eq!(parse_link(&format!("  {link}\n")), Some(("192.168.0.7:4242".to_string(), key)));
    }

    #[test]
    fn bad_links_are_refused() {
        let key = format_link("h:1", &[7; 32]);
        let code = key.split_once('#').unwrap().1;
        for link in [
            "",
            "pearl://",
            "http://a:1#X",
            "pearl://a:1",
            "pearl://a#AAAA",
            "pearl://:1#AAAA",
            "pearl://a:99999#AAAA",
            "pearl://a:1#!!!",
            "pearl://a:1#AAAA",
            &format!("pearl://a:notaport#{code}"),
            &format!("pearl://a:1#{code}{code}"),
        ] {
            assert_eq!(parse_link(link), None, "{link}");
        }
        assert!(parse_link(&format!("pearl://a:1#{code}")).is_some());
    }

    #[test]
    fn messages_round_trip() {
        let c = CursorInfo { id: 9, name: "Ana".into(), anchor: vec![1], head: vec![2, 3] };
        for m in [
            Message::Hello { name: "Ana".into() },
            Message::Welcome { file_name: "a.typ".into(), state: vec![1, 2] },
            Message::Update(vec![3]),
            Message::Cursor(c),
            Message::AskCursors,
            Message::Leave { id: 5 },
            Message::File { path: "a/b.png".into(), bytes: vec![0; 10] },
        ] {
            assert_eq!(Message::decode(&m.encode()), Some(m));
        }
    }

    #[test]
    fn hostile_messages_decode_to_nothing_without_panicking() {
        let good = Message::Welcome { file_name: "a.typ".into(), state: vec![1, 2, 3] }.encode();
        for cut in 0..good.len() {
            assert_eq!(Message::decode(good.get(..cut).unwrap()), None, "{cut}");
        }
        assert_eq!(Message::decode(&[200]), None);
        // A length far past the end of the message.
        assert_eq!(Message::decode(&[3, 0xFF, 0xFF, 0xFF, 0xFF, 1]), None);
    }

    #[test]
    fn received_files_cannot_leave_the_session_folder() {
        let root = Path::new("/tmp/s");
        assert_eq!(safe_path(root, "fig/a.png"), Some(root.join("fig").join("a.png")));
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a//b", "./a", "a\\..\\b", "C:/x", "a/", "a\0b"] {
            assert_eq!(safe_path(root, bad), None, "{bad}");
        }
    }

    #[test]
    fn the_hosts_file_name_is_reduced_to_a_plain_name() {
        assert_eq!(safe_file_name("tese.typ"), "tese.typ");
        assert_eq!(safe_file_name("../../etc/tese.typ"), "tese.typ");
        assert_eq!(safe_file_name("..\\x.typ"), "x.typ");
        for bad in ["", ".typ", "a.exe", "x:y.typ", "pasta/"] {
            assert_eq!(safe_file_name(bad), "documento.typ", "{bad}");
        }
    }

    #[test]
    fn names_are_one_short_line() {
        assert_eq!(clean_name("  Ana\n"), "Ana");
        assert_eq!(clean_name("\n"), "Convidado");
        assert_eq!(clean_name(&"x".repeat(500)).chars().count(), MAX_NAME);
    }
}
