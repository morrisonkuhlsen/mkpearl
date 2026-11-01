//! The terms of everything inside the program, which has to travel with it: a copy of the program
//! may be a lone file, so the text lives in it.

use std::sync::OnceLock;

pub const TEXT: &str = include_str!("../THIRD-PARTY-LICENSES.txt");

/// The text by line, to show only what is in view of something this long.
pub fn lines() -> &'static [&'static str] {
    static LINES: OnceLock<Vec<&'static str>> = OnceLock::new();
    LINES.get_or_init(|| TEXT.lines().collect())
}
