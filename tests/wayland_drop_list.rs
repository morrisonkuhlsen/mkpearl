//! The list of files a Wayland drag sends comes from another program, so it is hostile input (#15).
#![cfg(unix)]

use std::path::PathBuf;

#[path = "../vendor/winit/src/platform_impl/linux/wayland/seat/uri_list.rs"]
#[allow(dead_code)]
mod uri_list;

fn paths(list: &str) -> Vec<PathBuf> {
    uri_list::files(list.as_bytes())
}

#[test]
fn reads_the_local_files_of_a_list() {
    let list = "# copied\r\nfile:///home/a/foto%20nova.png\r\nfile://localhost/tmp/b.jpg\nhttps://example.com/c.png\nfile://other/d.png\n\n";
    assert_eq!(paths(list), [PathBuf::from("/home/a/foto nova.png"), PathBuf::from("/tmp/b.jpg")]);
}

#[test]
fn decodes_accents_and_leaves_broken_escapes_as_they_are() {
    assert_eq!(paths("file:///tmp/caf%C3%A9.png"), [PathBuf::from("/tmp/café.png")]);
    assert_eq!(paths("file:///a%2"), [PathBuf::from("/a%2")]);
    assert_eq!(paths("file:///a%"), [PathBuf::from("/a%")]);
    assert_eq!(paths("file:///a%zz"), [PathBuf::from("/a%zz")]);
    assert_eq!(paths("file:///a%é"), [PathBuf::from("/a%é")]);
}

#[test]
fn garbage_is_no_file_and_no_crash() {
    assert!(paths("").is_empty());
    assert!(paths("file://").is_empty());
    assert!(paths("file:").is_empty());
    assert!(uri_list::files(&[0xff, 0xfe, b'\n', 0, b'%']).is_empty());
    let names_with_bad_bytes = uri_list::files(b"file:///a\xff%ff");
    assert_eq!(names_with_bad_bytes.len(), 1);
}
