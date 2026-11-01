//! What hasn't been saved survives the program ending badly: a copy of the changed text, in a file of
//! this run under the user's data folder, readable by this user alone. A run holds a lock on a file
//! beside it, which the system lets go of however the run ends, so a copy whose lock is free has
//! nobody left to look after it. A clean exit leaves nothing behind.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::history;

fn lock_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.lock"))
}

fn text_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.typ"))
}

fn temporary_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.tmp"))
}

fn remove_all(dir: &Path, id: &str) {
    let _ = fs::remove_file(text_of(dir, id));
    let _ = fs::remove_file(temporary_of(dir, id));
    let _ = fs::remove_file(lock_of(dir, id));
}

/// This run's copy.
pub struct Recovery {
    dir: PathBuf,
    id: String,
    _lock: File,
    written: bool,
}

impl Recovery {
    pub fn start(dir: &Path) -> std::io::Result<Self> {
        history::private_folder(dir)?;
        let id = format!("{}-{}", std::process::id(), history::now_ms());
        let lock = OpenOptions::new().create(true).write(true).truncate(true).open(lock_of(dir, &id))?;
        lock.try_lock().map_err(std::io::Error::other)?;
        Ok(Self { dir: dir.to_path_buf(), id, _lock: lock, written: false })
    }

    /// Replaces the copy all at once, and on the disk before this returns: the end this is for may be
    /// the machine's.
    pub fn write(&mut self, document: Option<&Path>, text: &str) -> std::io::Result<()> {
        let head = serde_json::json!({ "document": document.map(|p| p.to_string_lossy()), "stamp": history::now_ms() });
        let temporary = temporary_of(&self.dir, &self.id);
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        writeln!(file, "{head}")?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, text_of(&self.dir, &self.id))?;
        self.written = true;
        Ok(())
    }

    /// Everything is saved: nothing to recover.
    pub fn clear(&mut self) {
        if std::mem::take(&mut self.written) {
            let _ = fs::remove_file(text_of(&self.dir, &self.id));
        }
    }

    pub fn is_written(&self) -> bool {
        self.written
    }

    /// The run is ending as it should.
    pub fn finish(self) {
        let Self { dir, id, _lock, .. } = self;
        drop(_lock);
        remove_all(&dir, &id);
    }
}

/// A copy left by a run that ended badly. Holding one keeps other runs from offering it too.
pub struct Orphan {
    pub document: Option<PathBuf>,
    pub text: String,
    pub stamp: u64,
    dir: PathBuf,
    id: String,
    lock: File,
}

impl Orphan {
    /// The copy has been dealt with.
    pub fn discard(self) {
        let Self { dir, id, lock, .. } = self;
        drop(lock);
        remove_all(&dir, &id);
    }
}

fn parse(contents: &str) -> Option<(Option<PathBuf>, u64, String)> {
    let (head, text) = contents.split_once('\n')?;
    let head: serde_json::Value = serde_json::from_str(head).ok()?;
    Some((head["document"].as_str().map(PathBuf::from), head["stamp"].as_u64()?, text.to_string()))
}

/// The copies nobody is looking after, newest first. Those with nothing in them to lose (no text, or
/// what the document's file already says) are cleared away here.
pub fn orphans(dir: &Path) -> Vec<Orphan> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut found = Vec::new();
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "lock")) {
        let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
        let Ok(lock) = OpenOptions::new().write(true).open(&path) else { continue };
        if lock.try_lock().is_err() {
            continue;
        }
        let contents = fs::read(text_of(dir, &id)).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
        let Some((document, stamp, text)) = contents.as_deref().and_then(parse) else {
            // Unreadable copies stay for someone to look at; a run that wrote none has nothing to keep.
            if contents.is_none() {
                drop(lock);
                remove_all(dir, &id);
            }
            continue;
        };
        let same_as_file = document.as_deref().is_some_and(|d| fs::read(d).is_ok_and(|on_disk| on_disk == text.as_bytes()));
        if same_as_file {
            drop(lock);
            remove_all(dir, &id);
            continue;
        }
        found.push(Orphan { document, text, stamp, dir: dir.to_path_buf(), id, lock });
    }
    found.sort_by_key(|o| std::cmp::Reverse(o.stamp));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pearl-recovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn count(dir: &Path) -> usize {
        fs::read_dir(dir).unwrap().count()
    }

    /// As a run that ended without cleaning up, with the lock let go of.
    fn abandon(r: Recovery) {
        drop(r);
    }

    #[test]
    fn a_living_run_is_left_alone_and_an_ended_one_is_offered() {
        let dir = folder("alive");
        let mut run = Recovery::start(&dir).unwrap();
        run.write(None, "texto\ncom linhas\n").unwrap();
        assert!(orphans(&dir).is_empty(), "this run is still going");
        abandon(run);
        let found = orphans(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "texto\ncom linhas\n");
        assert_eq!(found[0].document, None);
        assert!(orphans(&dir).is_empty(), "held by whoever is offering it");
        drop(found);
        let again = orphans(&dir);
        assert_eq!(again.len(), 1, "not dealt with, so offered next time");
        again.into_iter().next().unwrap().discard();
        assert_eq!(count(&dir), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_clean_exit_and_a_saved_document_leave_nothing() {
        let dir = folder("clean");
        let mut run = Recovery::start(&dir).unwrap();
        run.write(None, "a").unwrap();
        run.clear();
        assert!(!run.is_written());
        assert_eq!(count(&dir), 1, "only the lock");
        abandon(run);
        assert!(orphans(&dir).is_empty());
        assert_eq!(count(&dir), 0, "a run that left no text is tidied away");

        let mut run = Recovery::start(&dir).unwrap();
        run.write(None, "a").unwrap();
        run.finish();
        assert_eq!(count(&dir), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_copy_the_file_already_matches_has_nothing_to_offer() {
        let dir = folder("same");
        let document = dir.join("doc.typ.saved");
        fs::write(&document, "igual").unwrap();
        let mut run = Recovery::start(&dir).unwrap();
        run.write(Some(&document), "igual").unwrap();
        abandon(run);
        assert!(orphans(&dir).is_empty());

        let mut run = Recovery::start(&dir).unwrap();
        run.write(Some(&document), "diferente").unwrap();
        abandon(run);
        let found = orphans(&dir);
        assert_eq!(found[0].document.as_deref(), Some(document.as_path()));
        assert_eq!(found[0].text, "diferente");
        drop(found);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn newest_first_and_an_unreadable_copy_is_not_deleted() {
        let dir = folder("order");
        for text in ["velho", "novo"] {
            let mut run = Recovery::start(&dir).unwrap();
            run.write(None, text).unwrap();
            abandon(run);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let found = orphans(&dir);
        assert_eq!(found.iter().map(|o| o.text.as_str()).collect::<Vec<_>>(), ["novo", "velho"]);
        for o in found {
            o.discard();
        }
        let run = Recovery::start(&dir).unwrap();
        let id = run.id.clone();
        fs::write(text_of(&dir, &id), "sem cabeçalho").unwrap();
        abandon(run);
        assert!(orphans(&dir).is_empty());
        assert!(text_of(&dir, &id).exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn only_this_user_can_read_a_copy() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("private");
        let mut run = Recovery::start(&dir).unwrap();
        run.write(None, "segredo").unwrap();
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(text_of(&dir, &run.id)).unwrap().permissions().mode() & 0o777, 0o600);
        run.finish();
        fs::remove_dir_all(&dir).unwrap();
    }
}
