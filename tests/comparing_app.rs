//! Comparing versions through the window: from the Versions pane, the marks follow the document, and
//! nothing is changed by looking.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(40), "{what}: {:?} / {:?}", h.state().comparison_summary(), h.state().text());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_version_is_compared_on_the_pages_and_nothing_changes() {
    let root = std::env::temp_dir().join(format!("pearl-comparing-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, "O gato preto dorme.\n").unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.open_side(6);
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the page", |a| a.page_count() == 1);

    // The document changes, and the version from when it opened is what it's compared with.
    let now = "O gato branco dorme.\n";
    h.state_mut().set_text(now);
    h.state_mut().view_version(0);
    h.run_steps(3);
    h.get_by_label("Comparar nas páginas").click();
    h.run_steps(3);
    until(&mut h, "the marks", |a| a.comparison_summary() == Some((1, 1, 0)));
    assert!(h.query_by_label("Antes").is_none(), "painted on the pages, not widgets");
    assert_eq!(h.state().text(), now, "looking changes nothing");

    // Going through the changes.
    h.get_by_label("Próxima").click();
    h.run_steps(2);
    h.get_by_label("Anterior").click();
    h.run_steps(2);

    // It follows the document as it changes.
    let more = "O gato branco dorme muito.\n";
    h.state_mut().set_text(more);
    until(&mut h, "the marks again", |a| a.comparison_summary() == Some((1, 2, 0)));

    // A version that matches the document has nothing to mark.
    h.get_by_label("Fechar").click();
    h.run_steps(2);
    assert_eq!(h.state().comparison_summary(), None);
    assert_eq!(h.state().text(), more);

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
