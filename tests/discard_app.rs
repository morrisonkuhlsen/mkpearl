//! Discarding is only true once something has replaced the document: until then the edits stay
//! unsaved, with their recovery copy.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::i18n::tx;

const TYPED: &str = "texto que nunca foi salvo";

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} {:?}", h.state().text(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Clicks a recent document in the File view and answers Discard.
fn discard_for_recent(h: &mut Harness<'_, App>, name: &str) {
    h.state_mut().open_file_view();
    h.run_steps(3);
    h.get_by_label(name).click();
    h.run_steps(2);
    h.get_by_label(tx("discard")).click();
    h.run_steps(3);
}

#[test]
fn discarding_for_nothing_keeps_the_edits_unsaved() {
    let root = std::env::temp_dir().join(format!("pearl-discard-{}", std::process::id()));
    std::fs::create_dir_all(root.join("config").join("pearl")).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let (gone, there) = (root.join("sumiu.typ"), root.join("existe.typ"));
    std::fs::write(&there, "outro documento\n").unwrap();
    let list = format!("{}\n{}\n", gone.display(), there.display());
    std::fs::write(root.join("config").join("pearl").join("recent.txt"), list).unwrap();

    let mut app = App::new(None).unwrap();
    app.enable_recovery();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(2);
    h.event(egui::Event::Text(TYPED.to_string()));
    wait(&mut h, "the recovery copy", |h| h.state().has_recovery_copy());

    // A recent file that's gone opens nothing, so nothing was discarded.
    discard_for_recent(&mut h, "sumiu.typ");
    assert!(h.state().text().contains(TYPED));
    assert!(h.state().is_dirty(), "the edits count as unsaved");
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(500) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.state().has_recovery_copy(), "the recovery copy stays");

    // One that opens replaces the document, and only then is the copy gone.
    discard_for_recent(&mut h, "existe.typ");
    assert_eq!(h.state().text(), "outro documento\n");
    assert!(!h.state().is_dirty());
    wait(&mut h, "the copy going", |h| !h.state().has_recovery_copy());
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
