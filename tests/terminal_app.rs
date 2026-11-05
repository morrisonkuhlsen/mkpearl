//! The terminal pane through the window: started by a click, it takes the keyboard from the window,
//! and what a program in it writes to the open file shows in the source, with the text before it
//! kept in the history.
#![cfg(unix)]

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::history::Store;
use pearl::i18n::tx;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?}", h.state().terminal_text(&h.ctx));
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn screen_has(h: &Harness<'_, App>, text: &str) -> bool {
    h.state().terminal_text(&h.ctx).is_some_and(|s| s.contains(text))
}

fn key(h: &mut Harness<'_, App>, key: Key, modifiers: Modifiers) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    h.step();
}

fn typed(h: &mut Harness<'_, App>, text: &str) {
    h.event(Event::Text(text.to_string()));
    h.step();
    key(h, Key::Enter, Modifiers::NONE);
}

#[test]
fn a_program_in_the_terminal_edits_the_document() {
    let root = std::env::temp_dir().join(format!("pearl-terminal-app-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("ENV", "/dev/null");
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, "antes\n").unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(file.clone())).unwrap(),
    );
    h.run_steps(2);

    // Nothing runs until asked: the rail opens the pane, and a click starts the shell.
    assert!(h.state().terminal_text(&h.ctx).is_none());
    h.get_by_label(tx("terminal")).click();
    h.run_steps(2);
    assert!(h.state().terminal_open());
    h.get_by_label(tx("shell")).click();
    h.run_steps(2);
    assert!(h.state().terminal_focused(&h.ctx), "the terminal takes the keyboard");

    // What's typed reaches the shell, in the document's folder; the editor gets none of it.
    typed(&mut h, "echo pearl-$((40+2)) && pwd");
    wait(&mut h, "the shell's answer", |h| screen_has(h, "pearl-42"));
    assert!(screen_has(&h, root.file_name().unwrap().to_str().unwrap()));
    assert_eq!(h.state().text(), "antes\n");

    // Ctrl+C, which the window reads as copy, interrupts the program when nothing is selected.
    typed(&mut h, "sleep 30; echo not-$((0+1))-interrupted");
    std::thread::sleep(Duration::from_millis(300));
    h.event(Event::Copy);
    h.step();
    typed(&mut h, "echo after-$((2*3))");
    wait(&mut h, "the prompt after Ctrl+C", |h| screen_has(h, "after-6"));
    assert!(!screen_has(&h, "not-1-interrupted"), "{:?}", h.state().terminal_text(&h.ctx));

    // A paste arrives whole.
    h.event(Event::Paste("echo colado-$((1+1))".to_string()));
    h.step();
    key(&mut h, Key::Enter, Modifiers::NONE);
    wait(&mut h, "the paste", |h| screen_has(h, "colado-2"));

    // What the program writes to the open file shows in the source, the old text in the history.
    typed(&mut h, "printf 'escrito pelo agente\\n' > doc.typ");
    wait(&mut h, "the agent's text", |h| h.state().text() == "escrito pelo agente\n");
    assert!(!h.state().is_dirty());
    let store = Store::for_document(&file).unwrap();
    let kept: Vec<String> = store.list().iter().filter_map(|v| store.read(v)).collect();
    assert!(kept.iter().any(|v| v == "antes\n"), "{kept:?}");
    assert!(h.state().terminal_focused(&h.ctx), "following the file doesn't take the keyboard");

    // Hiding the pane leaves the program running; ending it ends the program.
    typed(&mut h, "echo $$ > pid");
    let started = Instant::now();
    let pid = loop {
        if let Some(pid) = std::fs::read_to_string(root.join("pid")).ok().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
            break pid;
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    };
    // Tab and Esc stay in the terminal; Ctrl+S is the program's, not a save.
    key(&mut h, Key::Tab, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(h.state().terminal_focused(&h.ctx));
    key(&mut h, Key::S, Modifiers::CTRL);
    key(&mut h, Key::Q, Modifiers::CTRL);
    assert!(!h.state().is_dirty());

    let alive = |pid: &str| std::process::Command::new("kill").args(["-0", pid]).status().is_ok_and(|s| s.success());
    h.get_by_label(tx("hide-the-terminal")).click();
    h.run_steps(2);
    assert!(!h.state().terminal_open());
    assert!(alive(&pid));
    h.state_mut().toggle_terminal();
    h.run_steps(2);
    h.get_by_label(tx("end-the-program")).click();
    h.run_steps(2);
    assert!(h.state().terminal_text(&h.ctx).is_none(), "ended");
    let started = Instant::now();
    while alive(&pid) {
        assert!(started.elapsed() < Duration::from_secs(10), "the shell outlived the terminal");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(h.state().terminal_text(&h.ctx).is_none());

    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
