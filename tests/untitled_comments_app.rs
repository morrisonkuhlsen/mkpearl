//! The comments of a document with no file stay with it when another document with no file comes in.

use egui_kittest::Harness;
use pearl::app::App;

fn window<'a>(app: App) -> Harness<'a, App> {
    Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

#[test]
fn a_new_untitled_document_starts_without_the_last_ones_comments() {
    let root = std::env::temp_dir().join(format!("pearl-untitled-comments-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let mut app = App::new(None).unwrap();
    app.open_side(9);
    let mut h = window(app);
    h.run_steps(5);
    let ctx = h.ctx.clone();
    h.state_mut().set_text("Um trecho para comentar.\n");
    h.run_steps(3);
    h.state_mut().select(&ctx, 3, 9);
    h.run_steps(3);
    assert!(h.state_mut().comment_on_selection(&ctx, "Reveja."));
    h.run_steps(3);
    assert_eq!(h.state().comment_threads().len(), 1);

    // Ctrl+N, then typing: nothing of the last document's comments, not even as lost passages.
    h.state_mut().start_document(&ctx, "Outro texto.\n".to_string());
    h.run_steps(3);
    h.state_mut().set_text("Outro texto, já mudado.\n");
    h.run_steps(3);
    assert!(h.state().comment_threads().is_empty(), "{:?}", h.state().comment_threads());
    let _ = std::fs::remove_dir_all(&root);
}
