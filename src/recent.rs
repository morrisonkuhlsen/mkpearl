//! The recently used documents, one path per line in the user's configuration folder. Losing this
//! list costs nothing, so a failure to read or write it is ignored.

use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 20;

fn config_file(name: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("pearl").join(name))
}

fn file() -> Option<PathBuf> {
    config_file("recent.txt")
}

/// A small setting kept in its own file; losing it costs nothing, like the list.
pub fn read_setting(name: &str) -> Option<String> {
    std::fs::read_to_string(config_file(name)?).ok()
}

pub fn write_setting(name: &str, value: &str) {
    let Some(f) = config_file(name) else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(f, value);
}

pub fn parse(text: &str) -> Vec<PathBuf> {
    text.lines().filter(|l| !l.trim().is_empty()).take(MAX_RECENT).map(PathBuf::from).collect()
}

pub fn load() -> Vec<PathBuf> {
    file().and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse(&t)).unwrap_or_default()
}

pub fn save(list: &[PathBuf]) {
    let Some(f) = file() else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let lines: Vec<String> = list.iter().map(|p| p.display().to_string()).collect();
    let _ = std::fs::write(f, lines.join("\n"));
}

/// A secret kept readable by this user alone. False when it couldn't be written that way.
pub fn write_secret(name: &str, value: &str) -> bool {
    use std::io::Write;
    let Some(f) = config_file(name) else { return false };
    if let Some(dir) = f.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return false;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let Ok(mut file) = options.open(&f) else { return false };
    // An older file keeps its permissions when opened: tighten them too.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.set_permissions(std::fs::Permissions::from_mode(0o600)).is_err() {
            return false;
        }
    }
    file.write_all(value.as_bytes()).is_ok()
}

pub fn remove_setting(name: &str) {
    if let Some(f) = config_file(name) {
        let _ = std::fs::remove_file(f);
    }
}

/// Moves `path` to the front. False when it already was there, so nothing need be written.
pub fn push(list: &mut Vec<PathBuf>, path: &Path) -> bool {
    if list.first().is_some_and(|p| p == path) {
        return false;
    }
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT);
    true
}

/// How long ago, in words.
pub fn age(seconds: u64) -> String {
    let plural = |n: u64, one: &str, many: &str| format!("há {n} {}", if n == 1 { one } else { many });
    match seconds {
        0..60 => "agora há pouco".to_string(),
        60..3_600 => plural(seconds / 60, "minuto", "minutos"),
        3_600..86_400 => plural(seconds / 3_600, "hora", "horas"),
        86_400..2_592_000 => plural(seconds / 86_400, "dia", "dias"),
        2_592_000..31_536_000 => plural(seconds / 2_592_000, "mês", "meses"),
        _ => plural(seconds / 31_536_000, "ano", "anos"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_moves_to_the_front_without_repeating_and_keeps_a_limit() {
        let mut list = parse("a.typ\n\nb.typ\n");
        assert_eq!(list.len(), 2);
        assert!(push(&mut list, Path::new("b.typ")));
        assert_eq!(list, [PathBuf::from("b.typ"), PathBuf::from("a.typ")]);
        assert!(!push(&mut list, Path::new("b.typ")));
        for i in 0..50 {
            push(&mut list, Path::new(&format!("{i}.typ")));
        }
        assert_eq!(list.len(), MAX_RECENT);
    }

    #[test]
    fn ages_read_in_portuguese() {
        assert_eq!(age(5), "agora há pouco");
        assert_eq!(age(60), "há 1 minuto");
        assert_eq!(age(7_200), "há 2 horas");
        assert_eq!(age(86_400), "há 1 dia");
        assert_eq!(age(5_184_000), "há 2 meses");
        assert_eq!(age(63_072_000), "há 2 anos");
    }
}
