//! The terminal with a real pseudoterminal: what's typed reaches the program, what it writes shows,
//! a program that isn't there says so, and ending the terminal leaves no process behind.
#![cfg(unix)]

use std::time::{Duration, Instant};

use egui::Color32;
use pearl::terminal::{Palette, Program, Terminal};

fn palette() -> Palette {
    Palette::new(false, Color32::BLACK, Color32::WHITE, Color32::BLUE)
}

fn wait_for(terminal: &mut Terminal, ctx: &egui::Context, what: &str) -> String {
    let started = Instant::now();
    loop {
        terminal.poll(ctx, &palette());
        let text = terminal.text(&palette());
        if text.contains(what) {
            return text;
        }
        assert!(started.elapsed() < Duration::from_secs(20), "waiting for {what:?}, the screen says:\n{text}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid]).status().is_ok_and(|s| s.success())
}

#[test]
fn a_shell_runs_and_ends_with_the_terminal() {
    let folder = std::env::temp_dir().join(format!("pearl-terminal-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    // SAFETY: set before any terminal starts, by the one test that reads it.
    unsafe {
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("ENV", "/dev/null");
    }
    let ctx = egui::Context::default();

    // A shell in the folder asked for, which answers what's typed.
    let mut shell = Terminal::start(&ctx, Program::Shell, None, &folder, (60, 10), (8, 16), "missing").unwrap();
    shell.input(b"cd . && echo pearl-$((40+2)) && echo $$ > pid && pwd\r".to_vec());
    let text = wait_for(&mut shell, &ctx, "pearl-42");
    assert!(text.contains(folder.file_name().unwrap().to_str().unwrap()), "{text}");
    let started = Instant::now();
    let pid = loop {
        if let Ok(pid) = std::fs::read_to_string(folder.join("pid"))
            && !pid.trim().is_empty()
        {
            break pid.trim().to_string();
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(alive(&pid));

    // Resizing reaches the program.
    shell.resize(40, 8, (8, 16));
    shell.input(b"stty size\r".to_vec());
    wait_for(&mut shell, &ctx, "8 40");

    // Ending the terminal ends its program.
    drop(shell);
    let started = Instant::now();
    while alive(&pid) {
        assert!(started.elapsed() < Duration::from_secs(10), "the shell {pid} outlived its terminal");
        std::thread::sleep(Duration::from_millis(20));
    }

    // One that won't hang up, or whose output Ctrl+S would have stopped, is ended all the same.
    let mut stubborn = Terminal::start(&ctx, Program::Shell, None, &folder, (60, 10), (8, 16), "missing").unwrap();
    let _ = std::fs::remove_file(folder.join("pid"));
    stubborn.input(b"trap '' HUP; echo $$ > pid; echo ready-$((1+1))\r".to_vec());
    wait_for(&mut stubborn, &ctx, "ready-2");
    stubborn.input(vec![0x13]);
    let pid = std::fs::read_to_string(folder.join("pid")).unwrap().trim().to_string();
    drop(stubborn);
    let started = Instant::now();
    while alive(&pid) {
        assert!(started.elapsed() < Duration::from_secs(10), "the stubborn shell {pid} outlived its terminal");
        std::thread::sleep(Duration::from_millis(20));
    }

    // A program that isn't installed says so, and a shell stays.
    let mut missing =
        Terminal::start(&ctx, Program::Claude, Some("pearl-no-such-program"), &folder, (80, 10), (8, 16), "not installed here").unwrap();
    wait_for(&mut missing, &ctx, "not installed here");
    missing.input(b"echo still-$((1+1))\r".to_vec());
    wait_for(&mut missing, &ctx, "still-2");

    // A program that ends is noticed.
    missing.input(b"exit 3\r".to_vec());
    let started = Instant::now();
    while missing.ended.is_none() {
        missing.poll(&ctx, &palette());
        assert!(started.elapsed() < Duration::from_secs(10), "the end wasn't noticed");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(missing.ended, Some(Some(3)));
    drop(missing);
    let _ = std::fs::remove_dir_all(&folder);
}
