//! Saving in answer to "save the changes?" can put the document first on the recent list. The document
//! that was clicked is still the one that opens (#40).

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::i18n::tx;

#[test]
fn the_clicked_document_opens_after_a_save_that_reorders_the_list() {
    let root = std::env::temp_dir().join(format!("pearl-save-then-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config").join("pearl")).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let (first, second, new) = (root.join("primeiro.typ"), root.join("segundo.typ"), root.join("novo.typ"));
    std::fs::write(&first, "o primeiro\n").unwrap();
    std::fs::write(&second, "o segundo\n").unwrap();
    std::fs::write(root.join("config").join("pearl").join("recent.txt"), format!("{}\n{}\n", first.display(), second.display())).unwrap();

    // A file that doesn't exist yet: it joins the list only when it's saved.
    let app = App::new(Some(new.clone())).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(2);
    h.event(egui::Event::Text("escrito aqui".to_string()));
    h.run_steps(1);
    // Or it would write the file by itself, and the question would never come.
    h.get_all_by_label(tx("autosave")).last().expect("the switch").click();
    h.run_steps(1);
    assert!(h.state().is_dirty(), "typed: {:?}", h.state().text());
    h.state_mut().open_file_view();
    h.run_steps(3);
    h.get_by_label("segundo.typ").click();
    h.run_steps(2);
    // The File view has a Save of its own, under the dialog's.
    h.get_all_by_label(tx("save")).last().expect("the dialog's button").click();

    let started = Instant::now();
    while !h.state().text().contains("o segundo") {
        assert!(started.elapsed() < Duration::from_secs(20), "opened instead: {:?}", h.state().text());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(std::fs::read_to_string(&new).unwrap().contains("escrito aqui"));
    let _ = std::fs::remove_dir_all(&root);
}
