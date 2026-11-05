//! Dark paper follows the setting and the theme, and doesn't touch the text.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;
use pearl::theme::ThemeChoice;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}");
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn dark_paper_needs_the_dark_theme_and_is_remembered() {
    let root = std::env::temp_dir().join(format!("pearl-dark-paper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let mut app = App::new(None).unwrap();
    app.start_with_theme(ThemeChoice::Dark);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    wait(&mut h, "the first compile", App::compiled_once);
    assert!(!h.state().on_dark_paper(), "off until asked for");
    let text = h.state().text().to_string();

    h.state_mut().set_dark_paper(true);
    wait(&mut h, "dark paper", App::on_dark_paper);
    assert_eq!(h.state().text(), text);

    let ctx = h.ctx.clone();
    h.state_mut().choose_theme(&ctx, ThemeChoice::Light);
    wait(&mut h, "white paper in the light theme", |app| !app.on_dark_paper());
    h.state_mut().choose_theme(&ctx, ThemeChoice::Dark);
    wait(&mut h, "dark paper again", App::on_dark_paper);
    drop(h);

    assert_eq!(std::fs::read_to_string(root.join("config").join("pearl").join("dark-paper.txt")).unwrap(), "on");
    let _ = std::fs::remove_dir_all(&root);
}
