//! The files named in a `text/uri-list`. Added for Pearl. The list comes from another program, so it
//! is hostile input: nothing here panics on it. Free of winit's types so Pearl's tests can include it.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// How much of a list is read. Past this it's no list of files anyone dragged.
pub const MAX_BYTES: usize = 1 << 20;

/// The local files in a list, in order; comments, other schemes and other hosts are left out.
pub fn files(list: &[u8]) -> Vec<PathBuf> {
    list.split(|&b| b == b'\n').filter_map(file).collect()
}

fn file(line: &[u8]) -> Option<PathBuf> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let rest = line.strip_prefix(b"file://")?;
    let rest = rest.strip_prefix(b"localhost").unwrap_or(rest);
    // Anything between the scheme and the first slash names another computer.
    if rest.first() != Some(&b'/') {
        return None;
    }
    Some(PathBuf::from(OsString::from_vec(percent_decoded(rest))))
}

fn percent_decoded(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some((&byte, after)) = rest.split_first() {
        let escaped = match after {
            [high, low, ..] if byte == b'%' => hex(*high).zip(hex(*low)),
            _ => None,
        };
        match (escaped, after.get(2..)) {
            (Some((high, low)), Some(tail)) => {
                out.push(high * 16 + low);
                rest = tail;
            }
            _ => {
                out.push(byte);
                rest = after;
            }
        }
    }
    out
}

fn hex(digit: u8) -> Option<u8> {
    (digit as char).to_digit(16).map(|d| d as u8)
}
