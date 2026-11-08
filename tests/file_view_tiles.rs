//! The File view keeps its templates on one row where the window has room for them.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::i18n::tx;

#[test]
fn the_templates_share_a_row_in_a_wide_window() {
    let mut app = App::new(None).unwrap();
    app.open_file_view();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(3);
    let tops: Vec<f32> =
        ["blank-document", "article", "report-abnt", "letter", "rsum"].iter().map(|id| h.get_by_label(tx(id)).rect().top()).collect();
    assert!(tops.iter().all(|top| *top == tops[0]), "{tops:?}");
}

#[test]
fn each_template_shows_its_own_first_page() {
    let mut app = App::new(None).unwrap();
    app.open_file_view();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_max_steps(u64::MAX)
        .with_accessibility_check(false)
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let started = std::time::Instant::now();
    while !h.state().template_thumbnails_ready() {
        assert!(started.elapsed() < std::time::Duration::from_secs(60), "the templates' pages never came");
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
