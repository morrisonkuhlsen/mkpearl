//! The ribbon's colour picker: what it writes into the document compiles, on the selected text.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::i18n::tx;

fn harness(text: &str) -> Harness<'static, App> {
    let mut app = App::new(None).unwrap();
    app.set_text(text);
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn select(h: &mut Harness<'_, App>, a: usize, b: usize) {
    let ctx = h.ctx.clone();
    h.state_mut().select(&ctx, a, b);
    h.run_steps(2);
}

fn open_text_colour(h: &mut Harness<'_, App>) {
    h.get_by_label(tx("text-colour")).click();
    h.run_steps(2);
}

/// Gives the worker time to compile what the document says now, and looks for errors.
fn compiles(h: &mut Harness<'_, App>) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().compiled_once());
    assert_eq!(h.state().error_count(), 0, "{}", h.state().text());
}

#[test]
fn the_picker_colours_the_selection_from_the_palette_and_by_code() {
    let root = std::env::temp_dir().join(format!("pearl-colour-picker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }

    let mut h = harness("um dois\n");
    select(&mut h, 0, 2);
    open_text_colour(&mut h);
    h.get_by_label("#2563eb").click();
    h.run_steps(3);
    assert_eq!(h.state().text(), "#text(fill: rgb(\"#2563eb\"))[um] dois\n");
    assert!(h.query_by_label(tx("palette")).is_none(), "the choice closes it");
    compiles(&mut h);

    // A code typed in the other tab, with an opacity: focus leaves the editor, the selection stays.
    let text = h.state().text().to_string();
    let start = text.find("dois").unwrap();
    select(&mut h, start, start + 4);
    open_text_colour(&mut h);
    h.get_by_label(tx("custom-colour")).click();
    h.run_steps(2);
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("#01020380".to_string()));
    h.run_steps(2);
    h.get_by_label(tx("apply")).click();
    h.run_steps(3);
    assert!(h.state().text().contains("#text(fill: rgb(\"#01020380\"))[dois]"), "{}", h.state().text());
    compiles(&mut h);

    // Clicking inside the picker doesn't close it, and it remembers what was used.
    open_text_colour(&mut h);
    h.get_by_label(tx("palette")).click();
    h.run_steps(2);
    assert!(h.query_by_label(tx("recent")).is_some());
    drop(h);
    let saved = std::fs::read_to_string(root.join("config").join("pearl").join("colours.txt")).unwrap();
    assert!(saved.contains("#2563eb"), "{saved}");
    let _ = std::fs::remove_dir_all(&root);
}
