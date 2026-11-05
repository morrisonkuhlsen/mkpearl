//! The time worked on a document comes back with it, and a window left alone adds nothing.

use egui_kittest::Harness;
use pearl::app::App;

fn harness(app: App) -> Harness<'static, App> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

/// Moves the pointer for a while, as someone working does.
fn work(h: &mut Harness<'_, App>, steps: u32) {
    for i in 0..steps {
        h.event(egui::Event::PointerMoved(egui::pos2(300.0 + i as f32, 300.0)));
        h.step();
    }
}

#[test]
fn the_time_survives_closing_and_idling_adds_nothing() {
    let root = std::env::temp_dir().join(format!("pearl-worktime-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, "= Texto\n").unwrap();

    let mut h = harness(App::new(Some(file.clone())).unwrap());
    work(&mut h, 40);
    let worked = h.state().worked_seconds();
    assert!(worked > 0, "working counts");

    // Frames with no input, for whatever reason they were drawn.
    for _ in 0..40 {
        h.step();
    }
    assert_eq!(h.state().worked_seconds(), worked, "a window left alone adds nothing");

    h.state_mut().save_worktime();
    drop(h);
    let mut h = harness(App::new(Some(file.clone())).unwrap());
    assert_eq!(h.state().worked_seconds(), worked, "another day, the same count");
    work(&mut h, 40);
    assert!(h.state().worked_seconds() > worked, "and it goes on from there");
    drop(h);

    assert_eq!(App::new(None).unwrap().worked_seconds(), 0, "a new document starts at nothing");
    let _ = std::fs::remove_dir_all(&root);
}
