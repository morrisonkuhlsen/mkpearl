//! A folder opened as a project: its tree, read on a thread of its own and read again every few
//! seconds, so files another program adds or changes show up, and the pages follow a change to a
//! file the document reads. A folder is hostile input: the walk is bounded in depth and in entries,
//! doesn't follow symbolic links, and skips what it can't read.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

/// How deep the tree goes, and how many entries it holds.
pub const MAX_DEPTH: usize = 8;
pub const MAX_ENTRIES: usize = 2_000;
/// How often the folder is read again.
const EVERY: Duration = Duration::from_secs(2);

/// One file or folder, by its path from the project's folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub depth: usize,
    pub dir: bool,
}

impl Entry {
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    pub fn is_typst(&self) -> bool {
        !self.dir && self.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("typ"))
    }
}

/// The folder as read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    /// Folders first, then files, each by name; a folder's contents follow it.
    pub entries: Vec<Entry>,
    /// More than `MAX_ENTRIES`, or deeper than `MAX_DEPTH`: some are left out.
    pub cut: bool,
    /// Changes when any file's size or time does.
    pub fingerprint: u64,
}

fn skipped(name: &str) -> bool {
    name.starts_with('.') || name == "target" || name == "node_modules"
}

/// Reads the tree under `root`.
pub fn read(root: &Path) -> Tree {
    let mut tree = Tree::default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for b in bytes {
            hash = (hash ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    walk(root, root, 0, &mut tree, &mut mix);
    tree.fingerprint = hash;
    tree
}

fn walk(root: &Path, dir: &Path, depth: usize, tree: &mut Tree, mix: &mut impl FnMut(&[u8])) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut here: Vec<(bool, String, PathBuf, Option<std::fs::Metadata>)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        // The entry itself, not where a link points: a link to a folder above would loop.
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            here.push((true, name, path, None));
        } else if kind.is_file() || (kind.is_symlink() && path.metadata().is_ok_and(|m| m.is_file())) {
            here.push((false, name, path, entry.metadata().ok()));
        }
        if here.len() > MAX_ENTRIES {
            tree.cut = true;
            break;
        }
    }
    here.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    for (dir_entry, _, path, meta) in here {
        if tree.entries.len() >= MAX_ENTRIES {
            tree.cut = true;
            return;
        }
        let Ok(relative) = path.strip_prefix(root) else { continue };
        mix(relative.as_os_str().as_encoded_bytes());
        if let Some(meta) = &meta {
            mix(&meta.len().to_le_bytes());
            let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            mix(&modified.to_le_bytes());
        }
        tree.entries.push(Entry { path: relative.to_path_buf(), depth, dir: dir_entry });
        if dir_entry {
            if depth + 1 < MAX_DEPTH {
                walk(root, &path, depth + 1, tree, mix);
            } else {
                tree.cut = true;
            }
        }
    }
}

/// Picks the document to open with a folder: `main.typ`, or the only Typst file at its top, or the
/// first by name there.
pub fn main_file(tree: &Tree) -> Option<PathBuf> {
    let top: Vec<&Entry> = tree.entries.iter().filter(|e| e.depth == 0 && e.is_typst()).collect();
    top.iter().find(|e| e.name().eq_ignore_ascii_case("main.typ")).or_else(|| top.first()).map(|e| e.path.clone())
}

/// The window's side of the thread that reads the folder.
pub struct Follower {
    to: Option<Sender<(Option<PathBuf>, egui::Context)>>,
    from: Option<Receiver<(PathBuf, Tree)>>,
}

impl Default for Follower {
    fn default() -> Self {
        Follower::new()
    }
}

impl Follower {
    pub fn new() -> Follower {
        let (to, inbox) = channel::<(Option<PathBuf>, egui::Context)>();
        let (outbox, from) = channel();
        match std::thread::Builder::new().name("project".into()).spawn(move || run(&inbox, &outbox)) {
            Ok(_) => Follower { to: Some(to), from: Some(from) },
            Err(_) => Follower { to: None, from: None },
        }
    }

    /// Reads `root` now and every few seconds after, or stops, for none.
    pub fn follow(&self, ctx: &egui::Context, root: Option<PathBuf>) {
        if let Some(to) = &self.to {
            let _ = to.send((root, ctx.clone()));
        }
    }

    /// The trees read since the last call, with the folder each is of.
    pub fn read(&self) -> Vec<(PathBuf, Tree)> {
        self.from.as_ref().map_or_else(Vec::new, |from| from.try_iter().collect())
    }
}

fn run(inbox: &Receiver<(Option<PathBuf>, egui::Context)>, outbox: &Sender<(PathBuf, Tree)>) {
    let mut root: Option<PathBuf> = None;
    let mut waker: Option<egui::Context> = None;
    let mut last: Option<u64> = None;
    loop {
        let wait = if root.is_some() { EVERY } else { Duration::from_secs(3600) };
        match inbox.recv_timeout(wait) {
            Ok((next, ctx)) => {
                root = next;
                waker = Some(ctx);
                last = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let Some(folder) = &root else { continue };
        let tree = read(folder);
        if last == Some(tree.fingerprint) {
            continue;
        }
        last = Some(tree.fingerprint);
        if outbox.send((folder.clone(), tree)).is_err() {
            return;
        }
        if let Some(ctx) = &waker {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pearl-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_come_first_and_hidden_ones_stay_out() {
        let dir = folder("order");
        std::fs::create_dir_all(dir.join("cap")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("cap/um.typ"), "").unwrap();
        std::fs::write(dir.join("Main.typ"), "").unwrap();
        std::fs::write(dir.join("a.bib"), "").unwrap();
        let tree = read(&dir);
        let paths: Vec<String> = tree.entries.iter().map(|e| e.path.to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(paths, ["cap", "cap/um.typ", "a.bib", "Main.typ"]);
        assert_eq!(main_file(&tree), Some(PathBuf::from("Main.typ")));
        assert!(!tree.cut);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_change_changes_the_fingerprint() {
        let dir = folder("print");
        std::fs::write(dir.join("a.typ"), "um").unwrap();
        let before = read(&dir).fingerprint;
        assert_eq!(read(&dir).fingerprint, before);
        std::fs::write(dir.join("a.typ"), "dois!").unwrap();
        assert_ne!(read(&dir).fingerprint, before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Thousands of files, deep folders and a link back up neither hang nor run without bound.
    #[test]
    fn hostile_folders_are_bounded() {
        let dir = folder("hostile");
        for i in 0..(MAX_ENTRIES + 300) {
            std::fs::write(dir.join(format!("f{i}.txt")), "").unwrap();
        }
        let mut deep = dir.join("d");
        for _ in 0..(MAX_DEPTH + 4) {
            deep = deep.join("d");
        }
        std::fs::create_dir_all(&deep).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.join("d").join("loop")).unwrap();
        let tree = read(&dir);
        assert!(tree.cut);
        assert!(tree.entries.len() <= MAX_ENTRIES);
        assert!(tree.entries.iter().all(|e| e.depth < MAX_DEPTH));
        assert!(!tree.entries.iter().any(|e| e.path.ends_with("loop")), "a link to a folder isn't followed");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_folder_is_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("closed");
        std::fs::create_dir_all(dir.join("fechada")).unwrap();
        std::fs::write(dir.join("fechada/x.typ"), "").unwrap();
        std::fs::set_permissions(dir.join("fechada"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let tree = read(&dir);
        assert!(tree.entries.iter().any(|e| e.path == Path::new("fechada")));
        std::fs::set_permissions(dir.join("fechada"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
