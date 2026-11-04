//! The editor's undo belongs to one document: opening another starts it over.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?}", h.state().text());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn press_undo(h: &mut Harness<'_, App>) {
    h.event(egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(3);
}

fn type_something(h: &mut Harness<'_, App>, words: &str) {
    h.run_steps(2);
    h.event(egui::Event::Text(words.to_string()));
    wait(h, "the typing", |a| a.text().contains(words));
}

#[test]
fn undo_never_brings_another_document_back() {
    let root = std::env::temp_dir().join(format!("pearl-undo-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let (first, second, third) = (root.join("a.typ"), root.join("b.typ"), root.join("c.typ"));
    std::fs::write(&first, "primeiro\n").unwrap();
    std::fs::write(&second, "segundo\n").unwrap();
    std::fs::write(&third, "terceiro\n").unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(first.clone())).unwrap(),
    );
    let ctx = h.ctx.clone();
    type_something(&mut h, "digitado em a ");

    // Another file: the shortcut finds nothing to undo, and so nothing reaches that file.
    h.state_mut().open_path(&ctx, second.clone());
    h.run_steps(3);
    press_undo(&mut h);
    assert_eq!(h.state().text(), "segundo\n");
    assert!(!h.state().is_dirty(), "an undo with nothing to undo is no edit");

    assert_eq!(std::fs::read_to_string(&second).unwrap(), "segundo\n", "the other document's text never reached this file");

    // The new document's own edits still undo, as far as the text it opened with.
    type_something(&mut h, "digitado em b ");
    press_undo(&mut h);
    assert_eq!(h.state().text(), "segundo\n");
    press_undo(&mut h);
    assert_eq!(h.state().text(), "segundo\n");

    // The title bar's button finds nothing either.
    h.state_mut().open_path(&ctx, third);
    h.run_steps(3);
    h.get_by_label("Desfazer (Ctrl+Z)").click();
    h.run_steps(3);
    assert_eq!(h.state().text(), "terceiro\n");
    assert!(!h.state().is_dirty());

    // A new document, with no file, starts over as well.
    h.state_mut().start_document(&ctx, "novo\n".to_string());
    h.run_steps(3);
    press_undo(&mut h);
    assert_eq!(h.state().text(), "novo\n");
    assert!(!h.state().is_dirty());

    // So does joining a session: once out of it, what was open before doesn't come back. The button,
    // since a guest's editor may not have the focus for the shortcut.
    let mut host = App::new(None).unwrap();
    host.set_text("do anfitrião\n");
    let link = host.host_session_on("127.0.0.1:0", "Ana").unwrap();
    let mut other = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        host,
    );
    h.state_mut().join_session_link(&ctx, &link, "Beto");
    let started = Instant::now();
    while h.state().text() != "do anfitrião\n" {
        assert!(started.elapsed() < Duration::from_secs(30), "the guest got the text: {:?}", h.state().text());
        h.step();
        other.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    h.state_mut().leave_session();
    h.get_by_label("Desfazer (Ctrl+Z)").click();
    h.run_steps(3);
    assert!(!h.state().text().contains("novo"), "{:?}", h.state().text());
    let _ = std::fs::remove_dir_all(&root);
}
