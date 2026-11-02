//! The autocompletion as it's used: typing, a list under the caret, keys that choose from it.

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
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

fn type_chars(h: &mut Harness<'_, App>, text: &str) {
    for c in text.chars() {
        type_text(h, &c.to_string());
    }
}

#[test]
fn brackets_close_as_typed_and_the_list_comes_inside_them() {
    let mut h = window();
    type_chars(&mut h, "#set text(");
    assert_eq!(h.state().text(), "#set text()");
    until(&mut h, "parameters inside the pair", |a| a.completion_labels().iter().any(|l| l == "fill"));
    h.key_press(Key::Escape);
    h.step();
    // Typing on goes inside, and the closer steps over the one already there.
    type_chars(&mut h, "size: 1pt)");
    assert_eq!(h.state().text(), "#set text(size: 1pt)");
}

#[test]
fn a_whole_call_typed_quickly_comes_out_right() {
    let mut h = window();
    // Several characters in one frame, brackets among them, stay in order.
    h.event(Event::Text("#".to_string()));
    h.event(Event::Text("f".to_string()));
    h.event(Event::Text("(".to_string()));
    h.event(Event::Text("x".to_string()));
    h.step();
    assert_eq!(h.state().text(), "#f(x)");
}

#[test]
fn backspace_takes_an_empty_pair() {
    let mut h = window();
    type_chars(&mut h, "x(");
    assert_eq!(h.state().text(), "x()");
    h.key_press(Key::Backspace);
    h.step();
    assert_eq!(h.state().text(), "x");
    h.key_press(Key::Backspace);
    h.step();
    assert_eq!(h.state().text(), "");
}

#[test]
fn a_typed_pair_counts_as_a_change() {
    let mut h = window();
    type_chars(&mut h, "(");
    assert_eq!(h.state().text(), "()");
    assert!(h.state().is_dirty());
}

#[test]
fn resting_the_pointer_on_a_function_shows_what_it_does() {
    let mut h = window();
    h.state_mut().set_text("#align(center)[texto]");
    until(&mut h, "a compile of it", |a| a.page_count() > 0);
    for _ in 0..6 {
        h.step();
    }
    // Over "align" on the first line: the frame, the rail, the line numbers, three characters in.
    let over = egui::pos2(8.0 + 44.0 + 46.0 + 4.0 + 3.5 * 7.8, 132.0 + 8.0 + 8.0);
    h.event(Event::PointerMoved(over));
    until(&mut h, "the tip", |a| a.hover_words().is_some());
    assert!(h.state().hover_words().unwrap().to_lowercase().contains("align"), "{:?}", h.state().hover_words());
    // Moving off the text takes it away.
    h.event(Event::PointerMoved(egui::pos2(300.0, 600.0)));
    h.step();
    h.step();
    assert_eq!(h.state().hover_words(), None);
}

fn searching<'a>(text: &str) -> Harness<'a, App> {
    let mut h = window();
    h.state_mut().set_text(text);
    h.step();
    h.state_mut().open_search();
    for _ in 0..4 {
        h.step();
    }
    h
}

#[test]
fn the_search_finds_as_it_is_typed() {
    let mut h = searching("uma Palavra aqui\ne palavra ali\n");
    h.event(Event::Text("palavra".to_string()));
    for _ in 0..3 {
        h.step();
    }
    let ctx = h.ctx.clone();
    assert_eq!(h.state().selected_text(&ctx), "Palavra", "selected without pressing anything, ignoring case");
    assert_eq!(h.state().selected_range(&ctx), (4, 11));
}

#[test]
fn enter_goes_to_the_next_match_and_wraps_round() {
    let mut h = searching("a x\nb x\nc x\n");
    h.event(Event::Text("x".to_string()));
    for _ in 0..3 {
        h.step();
    }
    let ctx = h.ctx.clone();
    let mut seen = vec![h.state().selected_range(&ctx)];
    for _ in 0..3 {
        h.key_press(Key::Enter);
        for _ in 0..3 {
            h.step();
        }
        seen.push(h.state().selected_range(&ctx));
    }
    assert_eq!(seen, [(2, 3), (6, 7), (10, 11), (2, 3)]);
    assert!(!h.state().text().contains("\n\n"), "Enter didn't reach the document");
}

#[test]
fn nothing_found_leaves_the_text_and_selection_alone() {
    let mut h = searching("abc");
    let ctx = h.ctx.clone();
    let before = h.state().selected_range(&ctx);
    h.event(Event::Text("zzz".to_string()));
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().selected_range(&ctx), before);
    assert_eq!(h.state().text(), "abc");
}

#[test]
fn a_ribbon_button_acts_on_the_selection_though_clicking_it_takes_the_focus() {
    use egui_kittest::kittest::Queryable as _;
    let mut h = window();
    h.state_mut().set_text("uma palavra aqui");
    h.step();
    let ctx = h.ctx.clone();
    h.state_mut().select(&ctx, 4, 11);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.state().selected_text(&ctx), "palavra");
    h.get_by_label("Negrito (Ctrl+B)").click();
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.state().text(), "uma *palavra* aqui");
}

#[test]
fn closing_the_search_leaves_the_match_selected_in_the_editor() {
    let mut h = searching("um alvo aqui");
    h.event(Event::Text("alvo".to_string()));
    for _ in 0..3 {
        h.step();
    }
    h.key_press(Key::Escape);
    for _ in 0..4 {
        h.step();
    }
    let ctx = h.ctx.clone();
    assert_eq!(h.state().selected_text(&ctx), "alvo");
    // Typing replaces it, as with any selection: the focus really came back.
    h.event(Event::Text("novo".to_string()));
    h.step();
    assert_eq!(h.state().text(), "um novo aqui");
}

#[test]
fn the_style_box_turns_a_line_into_a_list_and_shows_it() {
    use egui_kittest::kittest::Queryable as _;
    let mut h = window();
    h.state_mut().set_text("item");
    h.step();
    let ctx = h.ctx.clone();
    h.state_mut().select(&ctx, 2, 2);
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Estilo da linha do cursor").click();
    for _ in 0..3 {
        h.step();
    }
    // The ribbon has a button of that name too; the menu's item is the last one drawn.
    h.get_all_by_label("Lista numerada").last().unwrap().click();
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.state().text(), "+ item");
    h.get_by_label("Estilo da linha do cursor").click();
    for _ in 0..3 {
        h.step();
    }
    h.get_by_label("Normal").click();
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.state().text(), "item");
}

fn press_keys(h: &mut Harness<'_, App>) {
    for _ in 0..4 {
        h.step();
    }
}

/// The search open on its replacing part, with this word found and the keyboard in the field for what replaces it.
fn replacing<'a>(text: &str, word: &str) -> Harness<'a, App> {
    let mut h = searching(text);
    h.event(Event::Text(word.to_string()));
    press_keys(&mut h);
    let ctx = h.ctx.clone();
    h.state_mut().open_replace(&ctx);
    press_keys(&mut h);
    h
}

#[test]
fn control_h_opens_it_without_erasing_a_letter() {
    let mut h = window();
    h.state_mut().set_text("uma palavra aqui");
    h.step();
    let ctx = h.ctx.clone();
    h.state_mut().select(&ctx, 4, 11);
    h.step();
    h.key_press_modifiers(Modifiers::COMMAND, Key::H);
    press_keys(&mut h);
    assert_eq!(h.state().text(), "uma palavra aqui", "Ctrl+H doesn't reach the editor as a backspace");
    assert!(h.query_by_label("Substituir tudo").is_some());
    h.event(Event::Text("termo".to_string()));
    press_keys(&mut h);
    h.get_by_label("Substituir tudo").click();
    press_keys(&mut h);
    assert_eq!(h.state().text(), "uma termo aqui", "it opened on the word selected, with the keyboard in the other field");
}

#[test]
fn enter_replaces_one_at_a_time_and_stays_in_the_field() {
    let mut h = replacing("x um x dois x", "x");
    h.event(Event::Text("yy".to_string()));
    press_keys(&mut h);
    let ctx = h.ctx.clone();
    assert_eq!(h.state().selected_range(&ctx), (0, 1), "the match on show is the one it'll replace");
    h.key_press(Key::Enter);
    press_keys(&mut h);
    assert_eq!(h.state().text(), "yy um x dois x");
    assert_eq!(h.state().selected_range(&ctx), (6, 7), "on to the next one");
    h.key_press(Key::Enter);
    press_keys(&mut h);
    assert_eq!(h.state().text(), "yy um yy dois x", "the keyboard stayed: Enter didn't go to the document");
    assert!(!h.state().text().contains('\n'));
}

#[test]
fn replacing_with_nothing_selected_shows_the_match_first() {
    let mut h = replacing("a x b", "x");
    let ctx = h.ctx.clone();
    h.state_mut().select(&ctx, 0, 0);
    press_keys(&mut h);
    // The last of its name: the first turns the replacing part on and off.
    h.get_all_by_label("Substituir").last().unwrap().click();
    press_keys(&mut h);
    assert_eq!(h.state().text(), "a x b", "nothing changes unseen");
    assert_eq!(h.state().selected_range(&ctx), (2, 3));
    h.get_all_by_label("Substituir").last().unwrap().click();
    press_keys(&mut h);
    assert_eq!(h.state().text(), "a  b", "now it's seen, and with nothing typed the match goes");
}

#[test]
fn replacing_all_is_one_undo_step_and_says_how_many() {
    let mut h = replacing("Alvo, alvo e ALVO. Só isto fica.\n", "alvo");
    h.event(Event::Text("gol".to_string()));
    press_keys(&mut h);
    h.get_by_label("Substituir tudo").click();
    press_keys(&mut h);
    assert_eq!(h.state().text(), "gol, gol e gol. Só isto fica.\n");
    assert!(h.state().status().contains('3'), "{}", h.state().status());
    h.key_press(Key::Escape);
    press_keys(&mut h);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    press_keys(&mut h);
    assert_eq!(h.state().text(), "Alvo, alvo e ALVO. Só isto fica.\n");
}

#[test]
fn with_no_match_the_buttons_do_nothing() {
    let mut h = replacing("nada aqui", "xis");
    h.get_by_label("Substituir tudo").click();
    press_keys(&mut h);
    assert_eq!(h.state().text(), "nada aqui");
    assert!(!h.state().is_dirty());
}
