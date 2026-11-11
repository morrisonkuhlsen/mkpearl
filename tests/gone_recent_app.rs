//! A recent document that is gone opens nothing, and the session goes on (#62).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

#[test]
fn a_gone_recent_document_leaves_the_session_alone() {
    let root = std::env::temp_dir().join(format!("pearl-gone-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config").join("pearl")).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let (here, gone) = (root.join("aqui.typ"), root.join("sumiu.typ"));
    std::fs::write(&here, "o daqui\n").unwrap();
    std::fs::write(root.join("config").join("pearl").join("recent.txt"), format!("{}\n", gone.display())).unwrap();

    let mut app = App::new(Some(here.clone())).unwrap();
    app.host_session_on("127.0.0.1:0", "Ana").unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(2);
    h.state_mut().open_file_view();
    h.run_steps(3);
    h.get_by_label("sumiu.typ").click();
    h.run_steps(3);

    assert!(h.state().in_session(), "the session ended; status: {:?}", h.state().status());
    assert!(h.state().text().contains("o daqui"));
    assert!(h.state().status().contains("sumiu.typ"), "status: {:?}", h.state().status());
    let _ = std::fs::remove_dir_all(&root);
}
