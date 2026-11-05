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
