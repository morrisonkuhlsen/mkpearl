//! The Windows program is a window of its own, with no console, and carries its icon and version.

/// From a Windows program's headers: its subsystem, and the size of its resources.
fn read_headers(program: &[u8]) -> Option<(u16, u32)> {
    let word = |at: usize| Some(u16::from_le_bytes(program.get(at..at + 2)?.try_into().ok()?));
    let long = |at: usize| Some(u32::from_le_bytes(program.get(at..at + 4)?.try_into().ok()?));
    let signature = usize::try_from(long(0x3c)?).ok()?;
    if program.get(signature..signature + 4)? != b"PE\0\0" {
        return None;
    }
    let optional = signature + 24;
    // PE32+, the only kind Pearl is built as: 64-bit fields put the directories further along.
    if word(optional)? != 0x20b {
        return None;
    }
    const RESOURCE_DIRECTORY: usize = 112 + 2 * 8;
    Some((word(optional + 68)?, long(optional + RESOURCE_DIRECTORY + 4)?))
}

#[test]
fn headers_are_read_from_where_they_are() {
    let mut program = vec![0; 0x400];
    program[0x3c] = 0x80;
    program[0x80..0x84].copy_from_slice(b"PE\0\0");
    let optional = 0x80 + 24;
    program[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    program[optional + 68..optional + 70].copy_from_slice(&2u16.to_le_bytes());
    program[optional + 132..optional + 136].copy_from_slice(&500u32.to_le_bytes());
    assert_eq!(read_headers(&program), Some((2, 500)));
    assert_eq!(read_headers(&program[..0x90]), None, "cut short");
    assert_eq!(read_headers(b"not a program"), None);
}

#[cfg(windows)]
#[test]
fn pearl_has_no_console_and_has_its_resources() {
    const WINDOWS_GUI: u16 = 2;
    let program = std::fs::read(env!("CARGO_BIN_EXE_pearl")).unwrap();
    let (subsystem, resources) = read_headers(&program).unwrap();
    assert_eq!(subsystem, WINDOWS_GUI, "a console opens with Pearl");
    assert!(resources > 0, "no icon or version");
}
