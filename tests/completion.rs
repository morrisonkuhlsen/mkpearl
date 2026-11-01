//! The autocompletion as it's used: typing, a list under the caret, keys that choose from it.

use std::time::{Duration, Instant};

use egui::{Event, Key};
use egui_kittest::Harness;
use pearl::app::App;

fn window<'a>() -> Harness<'a, App> {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(None).unwrap(),
    );
    until(&mut h, "the first compile", |a| a.compiled_once());
    h.state_mut().set_text("");
    for _ in 0..5 {
        h.step();
    }
    h
}

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: text {:?}, list {:?}", h.state().text(), h.state().completion_labels());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn type_text(h: &mut Harness<'_, App>, text: &str) {
    h.event(Event::Text(text.to_string()));
    h.step();
}

#[test]
fn typing_shows_a_list_and_enter_takes_the_choice() {
    let mut h = window();
    type_text(&mut h, "#ali");
    until(&mut h, "the list", |a| a.completion_labels().iter().any(|l| l == "align"));
    let first = h.state().completion_labels()[0].clone();
    assert_eq!(first, "align", "the closest match comes first");
    h.key_press(Key::Enter);
    h.step();
    assert!(h.state().text().starts_with("#align("), "{:?}", h.state().text());
    assert!(!h.state().text().contains('\n'), "Enter went to the list, not the text");
    assert!(h.state().completion_labels().is_empty());
    // The caret landed inside the parentheses: what's typed next goes there.
    type_text(&mut h, "center");
    assert!(h.state().text().contains("align(center"), "{:?}", h.state().text());
}

#[test]
fn arrows_move_through_the_list_and_tab_also_accepts() {
    let mut h = window();
    type_text(&mut h, "#set text(");
    until(&mut h, "the list", |a| a.completion_labels().len() > 2);
    let second = h.state().completion_labels()[1].clone();
    h.key_press(Key::ArrowDown);
    h.step();
    h.key_press(Key::Tab);
    h.step();
    assert!(h.state().text().contains(&second), "{second} in {:?}", h.state().text());
    assert!(!h.state().text().contains('\t'), "Tab went to the list");
}

#[test]
fn escape_closes_the_list_and_keeps_the_text() {
    let mut h = window();
    type_text(&mut h, "#ali");
    until(&mut h, "the list", |a| !a.completion_labels().is_empty());
    h.key_press(Key::Escape);
    h.step();
    assert!(h.state().completion_labels().is_empty());
    assert_eq!(h.state().text(), "#ali");
    // With no list, Enter is a new line again.
    h.key_press(Key::Enter);
    h.step();
    assert_eq!(h.state().text(), "#ali\n");
}

#[test]
fn moving_the_caret_closes_the_list() {
    let mut h = window();
    type_text(&mut h, "#ali");
    until(&mut h, "the list", |a| !a.completion_labels().is_empty());
    h.key_press(Key::ArrowLeft);
    h.step();
    h.step();
    assert!(h.state().completion_labels().is_empty());
}

#[test]
fn prose_shows_no_list_and_ctrl_space_asks_for_one() {
    let mut h = window();
    type_text(&mut h, "texto comum");
    for _ in 0..40 {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().completion_labels().is_empty(), "{:?}", h.state().completion_labels());
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::Space);
    h.step();
    until(&mut h, "the list on request", |a| !a.completion_labels().is_empty());
}

#[test]
fn a_letter_typed_while_the_list_is_up_is_replaced_too() {
    let mut h = window();
    type_text(&mut h, "#ali");
    until(&mut h, "the list", |a| !a.completion_labels().is_empty());
    h.event(Event::Text("g".to_string()));
    h.step();
    h.key_press(Key::Enter);
    h.step();
    assert!(h.state().text().starts_with("#align("), "{:?}", h.state().text());
    assert!(!h.state().text().contains("alig("), "{:?}", h.state().text());
}
