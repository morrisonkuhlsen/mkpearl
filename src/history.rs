//! Every document's past versions, kept outside its folder so they don't clutter it or travel with
//! it: whole copies of the source, in a folder for that document under the user's data folder,
//! readable by this user alone. Older versions thin out with age.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const HOUR: u64 = 3_600_000;
const DAY: u64 = 24 * HOUR;
/// However old, this many of the newest stay.
const ALWAYS_KEEP: usize = 10;
const MAX_VERSIONS: usize = 300;
const MAX_BYTES: u64 = 100 * 1024 * 1024;
/// Past this a line comparison would take too long; the versions then count as wholly different.
const MAX_DIFF_CELLS: usize = 4_000_000;

/// Why a version was kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Opened,
    Saved,
    Autosaved,
    BeforeRestore,
    BeforeClaude,
    AfterClaude,
}

impl Kind {
    const ALL: [Kind; 6] = [Kind::Opened, Kind::Saved, Kind::Autosaved, Kind::BeforeRestore, Kind::BeforeClaude, Kind::AfterClaude];

    fn code(self) -> &'static str {
        match self {
            Kind::Opened => "opened",
            Kind::Saved => "saved",
            Kind::Autosaved => "auto",
            Kind::BeforeRestore => "restore",
            Kind::BeforeClaude => "before-claude",
            Kind::AfterClaude => "after-claude",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Opened => "Ao abrir",
            Kind::Saved => "Salvo",
            Kind::Autosaved => "Salvamento automático",
            Kind::BeforeRestore => "Antes de restaurar",
            Kind::BeforeClaude => "Antes do Claude",
            Kind::AfterClaude => "Depois do Claude",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// Milliseconds since 1970; identifies it within the document.
    pub stamp: u64,
    pub kind: Kind,
    pub bytes: u64,
    path: PathBuf,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The user's folder for data the program keeps.
pub fn data_folder() -> Option<PathBuf> {
    let from = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    from("XDG_DATA_HOME").or_else(|| from("APPDATA")).or_else(|| from("HOME").map(|h| h.join(".local").join("share")))
}

/// A hash that stays the same from one version of the program to the next (the standard library's
/// doesn't promise to), naming each document's folder.
fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

pub struct Store {
    dir: PathBuf,
}

pub(crate) fn private_folder(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

impl Store {
    /// The history of the document at `path`, under `base`. Moving or renaming the file starts a new one.
    pub fn at(base: &Path, path: &Path) -> Store {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let hash = stable_hash(canonical.to_string_lossy().as_bytes());
        Store { dir: base.join("pearl").join("history").join(format!("{hash:016x}")) }
    }

    /// The history of the document at `path`, in the usual place.
    pub fn for_document(path: &Path) -> Option<Store> {
        Some(Store::at(&data_folder()?, path))
    }

    /// The versions, newest first. A file that isn't one of ours is ignored.
    pub fn list(&self) -> Vec<Version> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut out: Vec<Version> = entries
            .flatten()
            .filter_map(|e| {
                let (stamp, kind) = parse_name(e.file_name().to_str()?)?;
                Some(Version { stamp, kind, bytes: e.metadata().ok()?.len(), path: e.path() })
            })
            .collect();
        out.sort_by_key(|v| std::cmp::Reverse(v.stamp));
        out
    }

    pub fn read(&self, version: &Version) -> Option<String> {
        std::fs::read_to_string(&version.path).ok()
    }

    /// Keeps `text` as a version at `now`, unless it's the same as the newest. True when kept.
    pub fn keep(&self, document: &Path, text: &str, kind: Kind, now: u64) -> std::io::Result<bool> {
        let versions = self.list();
        if let Some(newest) = versions.first()
            && newest.bytes == text.len() as u64
            && self.read(newest).as_deref() == Some(text)
        {
            return Ok(false);
        }
        private_folder(&self.dir)?;
        // A clock set back, or two within a millisecond, mustn't overwrite or reorder.
        let stamp = versions.first().map_or(now, |v| now.max(v.stamp + 1));
        let name = format!("{stamp:020}-{}.typ", kind.code());
        // Written whole and then moved in, so a crash leaves no half version.
        let partial = self.dir.join(format!("{name}.part"));
        let mut file = std::fs::File::create(&partial)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&partial, self.dir.join(&name))?;
        let _ = std::fs::write(self.dir.join("document.txt"), document.display().to_string());
        self.thin(now);
        Ok(true)
    }

    fn thin(&self, now: u64) {
        let versions = self.list();
        let sizes: Vec<(u64, u64)> = versions.iter().map(|v| (v.stamp, v.bytes)).collect();
        let removed = to_remove(&sizes, now);
        for version in versions.iter().filter(|v| removed.contains(&v.stamp)) {
            let _ = std::fs::remove_file(&version.path);
        }
    }

    /// Deletes the document's whole history. Can't be undone.
    pub fn clear(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

fn parse_name(name: &str) -> Option<(u64, Kind)> {
    let (stamp, rest) = name.strip_suffix(".typ")?.split_once('-')?;
    let kind = Kind::ALL.into_iter().find(|k| k.code() == rest)?;
    Some((stamp.parse().ok()?, kind))
}

/// Which versions go, from (stamp, bytes) newest first: all stay for a day, then one per hour for a
/// week, then one per day for three months; after that only the newest few, and never so many or so
/// large as to pass the limits.
pub fn to_remove(versions: &[(u64, u64)], now: u64) -> HashSet<u64> {
    let mut removed = HashSet::new();
    let mut buckets = HashSet::new();
    for (i, (stamp, _)) in versions.iter().enumerate() {
        if i < ALWAYS_KEEP {
            continue;
        }
        let age = now.saturating_sub(*stamp);
        let bucket = if age < DAY {
            None
        } else if age < 7 * DAY {
            Some((0, stamp / HOUR))
        } else if age < 90 * DAY {
            Some((1, stamp / DAY))
        } else {
            removed.insert(*stamp);
            continue;
        };
        if let Some(bucket) = bucket
            && !buckets.insert(bucket)
        {
            removed.insert(*stamp);
        }
    }
    let (mut count, mut total) = (0, 0);
    for (stamp, bytes) in versions {
        if removed.contains(stamp) {
            continue;
        }
        count += 1;
        total += bytes;
        if count > ALWAYS_KEEP && (count > MAX_VERSIONS || total > MAX_BYTES) {
            removed.insert(*stamp);
        }
    }
    removed
}

/// When, in words: "Hoje, 14:32", "Ontem, 09:05" or a date.
pub fn describe(stamp: u64, now: u64) -> String {
    use chrono::{Local, TimeZone};
    let local = |ms: u64| Local.timestamp_millis_opt(i64::try_from(ms).unwrap_or(0)).single();
    let (Some(when), Some(today)) = (local(stamp), local(now)) else { return "Data desconhecida".to_string() };
    let days = (today.date_naive() - when.date_naive()).num_days();
    match days {
        0 => format!("Hoje, {}", when.format("%H:%M")),
        1 => format!("Ontem, {}", when.format("%H:%M")),
        _ => when.format("%d/%m/%Y, %H:%M").to_string(),
    }
}

/// One changed line between two versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Added(String),
    Removed(String),
}

/// The lines that differ going from `old` to `new`, in order.
pub fn line_changes(old: &str, new: &str) -> Vec<Change> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let start = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let (rest_a, rest_b) = (&a[start..], &b[start..]);
    let end = rest_a.iter().rev().zip(rest_b.iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (&rest_a[..rest_a.len() - end], &rest_b[..rest_b.len() - end]);
    let added = |l: &&str| Change::Added((*l).to_string());
    let removed = |l: &&str| Change::Removed((*l).to_string());
    if a.len().saturating_mul(b.len()) > MAX_DIFF_CELLS {
        return a.iter().map(removed).chain(b.iter().map(added)).collect();
    }
    // Longest common subsequence, from the end, then walked from the start.
    let width = b.len() + 1;
    let mut table = vec![0_u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * width + j] = if a[i] == b[j] { table[(i + 1) * width + j + 1] + 1 } else { table[(i + 1) * width + j].max(table[i * width + j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            out.push(removed(&a[i]));
            i += 1;
        } else {
            out.push(added(&b[j]));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(removed));
    out.extend(b[j..].iter().map(added));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Temp {
            let dir = std::env::temp_dir().join(format!("pearl-history-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Temp(dir)
        }
        fn store(&self) -> Store {
            Store::at(&self.0, Path::new("/x/doc.typ"))
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn versions_come_back_newest_first_with_accents_intact() {
        let t = Temp::new("order");
        let s = t.store();
        assert!(s.keep(Path::new("/x/doc.typ"), "primeira ação", Kind::Opened, 100).unwrap());
        assert!(s.keep(Path::new("/x/doc.typ"), "segunda", Kind::Saved, 200).unwrap());
        let list = s.list();
        assert_eq!(list.iter().map(|v| (v.stamp, v.kind)).collect::<Vec<_>>(), [(200, Kind::Saved), (100, Kind::Opened)]);
        assert_eq!(s.read(&list[1]).as_deref(), Some("primeira ação"));
    }

    #[test]
    fn the_same_text_twice_is_kept_once() {
        let t = Temp::new("same");
        let s = t.store();
        assert!(s.keep(Path::new("d"), "a", Kind::Opened, 1).unwrap());
        assert!(!s.keep(Path::new("d"), "a", Kind::Saved, 2).unwrap());
        assert!(s.keep(Path::new("d"), "b", Kind::Saved, 3).unwrap());
        assert!(s.keep(Path::new("d"), "a", Kind::Saved, 4).unwrap(), "going back to older text is a new version");
        assert_eq!(s.list().len(), 3);
    }

    #[test]
    fn a_clock_set_back_neither_overwrites_nor_reorders() {
        let t = Temp::new("clock");
        let s = t.store();
        s.keep(Path::new("d"), "a", Kind::Saved, 1000).unwrap();
        s.keep(Path::new("d"), "b", Kind::Saved, 5).unwrap();
        s.keep(Path::new("d"), "c", Kind::Saved, 1001).unwrap();
        let texts: Vec<String> = s.list().iter().map(|v| s.read(v).unwrap()).collect();
        assert_eq!(texts, ["c", "b", "a"]);
    }

    #[test]
    fn each_document_has_its_own_history() {
        let t = Temp::new("own");
        let (a, b) = (Store::at(&t.0, Path::new("/x/a.typ")), Store::at(&t.0, Path::new("/x/b.typ")));
        a.keep(Path::new("a"), "a", Kind::Saved, 1).unwrap();
        assert!(b.list().is_empty());
        assert_eq!(a.list().len(), 1);
    }

    #[test]
    fn the_folder_name_never_changes_for_a_path() {
        // Saved histories must still be found after an update: this value is fixed on purpose.
        assert_eq!(stable_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(stable_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    #[cfg(unix)]
    fn the_history_is_private_to_this_user() {
        use std::os::unix::fs::PermissionsExt;
        let t = Temp::new("private");
        let s = t.store();
        s.keep(Path::new("d"), "segredo", Kind::Saved, 1).unwrap();
        assert_eq!(std::fs::metadata(&s.dir).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn stray_files_are_ignored_and_clearing_removes_everything() {
        let t = Temp::new("stray");
        let s = t.store();
        s.keep(Path::new("d"), "a", Kind::Saved, 1).unwrap();
        for stray in ["notas.txt", "x-saved.typ", "12-unknown.typ", "00000000000000000002-saved.typ.part"] {
            std::fs::write(s.dir.join(stray), "x").unwrap();
        }
        assert_eq!(s.list().len(), 1);
        s.clear().unwrap();
        assert!(s.list().is_empty());
        s.clear().unwrap();
    }

    #[test]
    fn old_versions_thin_out_with_age() {
        let now = 400 * DAY;
        let versions = |ages: &[u64]| -> Vec<(u64, u64)> { ages.iter().map(|age| (now - age, 10)).collect() };
        // Keep the newest ten out of the way: these are the fresh ones.
        let fresh: Vec<u64> = (1..=10).map(|i| i * 1000).collect();
        let with = |old: &[u64]| {
            let mut all = fresh.clone();
            all.extend_from_slice(old);
            versions(&all)
        };
        // Within a day, all stay.
        assert!(to_remove(&with(&[2 * HOUR, 2 * HOUR + 1, 2 * HOUR + 2]), now).is_empty());
        // Over a day old, one per hour: the newest in that hour.
        let base = 2 * DAY - (2 * DAY) % HOUR;
        let removed = to_remove(&with(&[base + 1, base + 2, base + 3]), now);
        assert_eq!(removed.len(), 2, "{removed:?}");
        assert!(!removed.contains(&(now - (base + 1))));
        // Over a week old, one per day.
        let base = 20 * DAY - (20 * DAY) % DAY;
        assert_eq!(to_remove(&with(&[base + 1, base + 2 * HOUR, base + 5 * HOUR]), now).len(), 2);
        // Past three months, gone.
        assert_eq!(to_remove(&with(&[100 * DAY]), now).len(), 1);
    }

    #[test]
    fn the_newest_stay_however_old_and_the_limits_hold() {
        let now = 1000 * DAY;
        let old: Vec<(u64, u64)> = (0..5).map(|i| (i * 1000 + 1, 10)).rev().collect();
        assert!(to_remove(&old, now).is_empty(), "only a few, however old");
        let many: Vec<(u64, u64)> = (0..400).map(|i| (now - i, 1)).collect();
        assert_eq!(to_remove(&many, now).len(), 100);
        let large: Vec<(u64, u64)> = (0..20).map(|i| (now - i, MAX_BYTES / 2)).collect();
        assert_eq!(to_remove(&large, now).len(), 10, "ten stay even over the size limit");
    }

    #[test]
    fn line_changes_find_what_differs() {
        let added = |s: &str| Change::Added(s.to_string());
        let removed = |s: &str| Change::Removed(s.to_string());
        assert!(line_changes("a\nb", "a\nb").is_empty());
        assert_eq!(line_changes("a\nb\nc", "a\nc"), [removed("b")]);
        assert_eq!(line_changes("a\nc", "a\nb\nc"), [added("b")]);
        assert_eq!(line_changes("a\nb\nc", "a\nx\nc"), [removed("b"), added("x")]);
        assert_eq!(line_changes("", "á"), [added("á")]);
        assert_eq!(line_changes("á", ""), [removed("á")]);
        assert_eq!(line_changes("x\na\ny\nb", "a\nz\nb"), [removed("x"), removed("y"), added("z")]);
    }

    #[test]
    fn huge_versions_compare_without_stalling() {
        let a: String = (0..5000).map(|i| format!("a{i}\n")).collect();
        let b: String = (0..5000).map(|i| format!("b{i}\n")).collect();
        assert_eq!(line_changes(&a, &b).len(), 10000);
    }

    #[test]
    fn times_read_in_words() {
        let now = now_ms();
        assert!(describe(now, now).starts_with("Hoje, "));
        assert!(describe(now - DAY, now).starts_with("Ontem, "));
        assert!(describe(now - 30 * DAY, now).contains('/'));
    }
}
