//! The open file, watched on a thread of its own so the window follows what other programs write to
//! it, such as a coding agent in a terminal. The thread only looks at the file's size and time, and
//! reads it only once both have held still for a moment: a burst of writes is one change, and a file
//! still being written is never taken as final. It wakes the window only when something changed, so
//! an idle window stays idle.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, SystemTime};

use crate::history::MAX_BYTES;

/// How often the file is looked at.
const EVERY: Duration = Duration::from_millis(250);

/// What became of the watched file.
#[derive(Debug, PartialEq, Eq)]
pub enum Seen {
    /// It holds something else now. Whether that is what Pearl itself wrote is the window's to tell.
    Changed { path: PathBuf, bytes: Vec<u8> },
    /// It was deleted or moved away.
    Gone { path: PathBuf },
    /// It changed, and is too large to read.
    TooLarge { path: PathBuf },
    /// It changed and can't be read.
    Unreadable { path: PathBuf, error: String },
}

/// What tells one state of the file from another without reading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stamp {
    Missing,
    Unreadable,
    Present { len: u64, modified: Option<SystemTime> },
}

fn stamp(path: &Path) -> Stamp {
    match std::fs::metadata(path) {
        Ok(meta) => Stamp::Present { len: meta.len(), modified: meta.modified().ok() },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stamp::Missing,
        Err(_) => Stamp::Unreadable,
    }
}

/// One file followed by the thread: the state last reported, and a new one waiting to hold still.
struct Followed {
    path: PathBuf,
    known: Stamp,
    settling: Option<Stamp>,
}

impl Followed {
    fn new(path: PathBuf) -> Followed {
        let known = stamp(&path);
        Followed { path, known, settling: None }
    }

    /// Looks once; a change is reported on the look after it was first seen, if nothing moved since.
    fn look(&mut self) -> Option<Seen> {
        let now = stamp(&self.path);
        if now == self.known {
            self.settling = None;
            return None;
        }
        if self.settling != Some(now) {
            self.settling = Some(now);
            return None;
        }
        self.settling = None;
        let path = self.path.clone();
        let seen = match now {
            Stamp::Missing => Seen::Gone { path },
            Stamp::Unreadable => Seen::Unreadable { path, error: String::new() },
            Stamp::Present { len, .. } if len > MAX_BYTES => Seen::TooLarge { path },
            Stamp::Present { len, .. } => match read_capped(&self.path) {
                // Written to while being read: not final yet, looked at again.
                Ok(bytes) if bytes.len() as u64 != len || stamp(&self.path) != now => {
                    self.settling = None;
                    return None;
                }
                Ok(bytes) => Seen::Changed { path, bytes },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Seen::Gone { path },
                Err(e) => Seen::Unreadable { path, error: e.to_string() },
            },
        };
        self.known = now;
        Some(seen)
    }
}

/// Reads at most `MAX_BYTES` and one more, so a file that grows while read can't take all memory.
fn read_capped(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_BYTES.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The window's side of the thread.
pub struct Watcher {
    to: Option<Sender<(Option<PathBuf>, egui::Context)>>,
    from: Option<Receiver<Seen>>,
    following: Option<Option<PathBuf>>,
}

impl Default for Watcher {
    fn default() -> Self {
        Watcher::new()
    }
}

impl Watcher {
    /// Starts the thread. If it can't start, the file just isn't followed: saving still notices.
    pub fn new() -> Watcher {
        let (to, inbox) = channel::<(Option<PathBuf>, egui::Context)>();
        let (outbox, from) = channel();
        let started = std::thread::Builder::new().name("watch".into()).spawn(move || run(&inbox, &outbox));
        match started {
            Ok(_) => Watcher { to: Some(to), from: Some(from), following: None },
            Err(_) => Watcher { to: None, from: None, following: None },
        }
    }

    /// Follows `path` from now on, or nothing. What the file says now is taken as known.
    pub fn follow(&mut self, ctx: &egui::Context, path: Option<&Path>) {
        let path = path.map(Path::to_path_buf);
        if self.following.as_ref() == Some(&path) {
            return;
        }
        if let Some(to) = &self.to {
            let _ = to.send((path.clone(), ctx.clone()));
        }
        self.following = Some(path);
    }

    /// What was seen since the last call.
    pub fn seen(&self) -> Vec<Seen> {
        self.from.as_ref().map_or_else(Vec::new, |from| from.try_iter().collect())
    }
}

fn run(inbox: &Receiver<(Option<PathBuf>, egui::Context)>, outbox: &Sender<Seen>) {
    let mut followed: Option<Followed> = None;
    let mut waker: Option<egui::Context> = None;
    loop {
        let wait = if followed.is_some() { EVERY } else { Duration::from_secs(3600) };
        match inbox.recv_timeout(wait) {
            Ok((path, ctx)) => {
                followed = path.map(Followed::new);
                waker = Some(ctx);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(seen) = followed.as_mut().and_then(Followed::look) {
                    if outbox.send(seen).is_err() {
                        return;
                    }
                    if let Some(ctx) = &waker {
                        ctx.request_repaint();
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pearl-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes with a time apart from the last, as filesystems with coarse times may not show.
    fn write(path: &Path, text: &[u8], second: u64) {
        std::fs::write(path, text).unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + second)).unwrap();
    }

    #[test]
    fn a_change_is_reported_once_it_holds_still() {
        let dir = folder("still");
        let file = dir.join("a.typ");
        write(&file, b"one", 1);
        let mut f = Followed::new(file.clone());
        assert_eq!(f.look(), None, "nothing changed");
        write(&file, b"two", 2);
        assert_eq!(f.look(), None, "first seen, not yet still");
        write(&file, b"three", 3);
        assert_eq!(f.look(), None, "still moving");
        assert_eq!(f.look(), Some(Seen::Changed { path: file.clone(), bytes: b"three".to_vec() }));
        assert_eq!(f.look(), None, "reported once");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn gone_too_large_and_back_are_each_reported() {
        let dir = folder("gone");
        let file = dir.join("a.typ");
        write(&file, b"one", 1);
        let mut f = Followed::new(file.clone());
        std::fs::remove_file(&file).unwrap();
        assert_eq!(f.look(), None);
        assert_eq!(f.look(), Some(Seen::Gone { path: file.clone() }));
        assert_eq!(f.look(), None);
        std::fs::File::create(&file).unwrap().set_len(MAX_BYTES + 1).unwrap();
        assert_eq!(f.look(), None);
        assert_eq!(f.look(), Some(Seen::TooLarge { path: file.clone() }));
        write(&file, &[0xff, 0xfe, 0x00], 5);
        assert_eq!(f.look(), None);
        assert_eq!(f.look(), Some(Seen::Changed { path: file.clone(), bytes: vec![0xff, 0xfe, 0x00] }));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_thread_reports_and_ends_with_the_window() {
        let dir = folder("thread");
        let file = dir.join("a.typ");
        write(&file, b"one", 1);
        let ctx = egui::Context::default();
        let mut watcher = Watcher::new();
        watcher.follow(&ctx, Some(&file));
        std::thread::sleep(EVERY);
        write(&file, b"two", 2);
        let started = std::time::Instant::now();
        let mut seen = Vec::new();
        while seen.is_empty() && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
            seen = watcher.seen();
        }
        assert_eq!(seen, vec![Seen::Changed { path: file.clone(), bytes: b"two".to_vec() }]);
        drop(watcher);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
