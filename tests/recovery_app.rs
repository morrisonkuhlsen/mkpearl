//! A run that really ends badly: a second process types and aborts, and this one recovers its text.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

const TYPED: &str = "texto que nunca foi salvo";

fn window(app: App) -> Harness<'static, App> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?}", h.state().text());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Runs only as the second process, started by the test below.
#[test]
fn second_process_types_and_aborts() {
    if std::env::var_os("PEARL_ABORT_AFTER_TYPING").is_none() {
        return;
    }
    let mut app = App::new(None).unwrap();
    app.enable_recovery();
    let mut h = window(app);
    wait(&mut h, "the first compile", |a| a.page_count() == 1);
    h.event(egui::Event::Text(TYPED.to_string()));
    wait(&mut h, "the copy", |a| a.has_recovery_copy());
    std::process::abort();
}

#[test]
fn what_a_crash_leaves_is_offered_and_recovered() {
    let root = std::env::temp_dir().join(format!("pearl-recovery-app-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: nothing else in this process reads them when the other test isn't the second process,
    // and that test returns at once without them being read.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "second_process_types_and_aborts", "--nocapture"])
        .env("PEARL_ABORT_AFTER_TYPING", "1")
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "the second process was to abort");

    let mut app = App::new(None).unwrap();
    app.enable_recovery();
    assert_eq!(app.recoverable(), ["Sem título"]);
    let mut h = window(app);
    wait(&mut h, "the first compile", |a| a.page_count() == 1);
    assert!(h.state().text() != TYPED && !h.state().is_dirty(), "nothing happens until asked");
    h.get_by_label("Recuperar").click();
    h.run_steps(3);
    assert!(h.state().text().contains(TYPED), "{:?}", h.state().text());
    assert!(h.state().is_dirty(), "recovered, but not saved anywhere yet");
    assert!(h.state().recoverable().is_empty());
    assert!(h.state().has_recovery_copy(), "still safe if this run ends badly too");

    // What the window showed before it isn't one undo away, to come back over the recovered text.
    let recovered = h.state().text().to_string();
    h.event(egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(3);
    assert_eq!(h.state().text(), recovered);

    h.state_mut().finish_recovery();
    let folder = root.join("data/pearl/recovery");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0, "a clean exit leaves nothing");
    let _ = std::fs::remove_dir_all(&root);
}
