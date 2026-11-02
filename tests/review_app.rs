//! Reviewing on the pages, with the pointer: a mark finds its source and becomes a suggestion, which is
//! accepted as one undo step or declined, and never applied to what has changed since.

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::app::comments::ReviewTool;
use pearl::compiler::PickRefusal;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} / {:?}", h.state().text(), h.state().comment_threads());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn press(h: &mut Harness<'_, App>, at: egui::Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}

fn click_at(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.event(Event::PointerMoved(at));
    h.step();
    press(h, at, true);
    press(h, at, false);
    h.step();
}

fn drag_over(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
    h.event(Event::PointerMoved(from));
    h.step();
    press(h, from, true);
    h.event(Event::PointerMoved(from + (to - from) * 0.5));
    h.step();
    h.event(Event::PointerMoved(to));
    h.step();
    press(h, to, false);
    h.step();
}

/// Where a word is in the window, found by leaving a comment on it and asking the pages.
fn on_screen(h: &mut Harness<'_, App>, word: &str) -> egui::Pos2 {
    let tool = h.state().review_tool();
    h.state_mut().set_review_tool(None);
    let ctx = h.ctx.clone();
    let text = h.state().text().to_string();
    let at = text[..text.find(word).unwrap()].chars().count();
    h.state_mut().select(&ctx, at, at + word.chars().count());
    h.run_steps(3);
    let before = h.state().comment_threads().len();
    assert!(h.state_mut().comment_on_selection(&ctx, "Só para achar."));
    h.state_mut().open_side(9);
    // The pages only have the open ones, and this one is the last of them.
    let last = h.state().comment_threads().iter().filter(|t| !t.2).count() - 1;
    until(h, "the word on the page", |a| a.comment_places().iter().any(|p| p.index == last));
    let place = *h.state().comment_places().iter().find(|p| p.index == last).unwrap();
    let page = h.state().page_rect(place.page).unwrap();
    let zoom = page.width() / 595.28;
    h.state_mut().delete_comment(before);
    h.state_mut().set_review_tool(tool);
    h.run_steps(3);
    page.min + egui::vec2(place.x + place.width / 2.0, place.y + place.height / 2.0) * zoom
}

const BODY: &str = "= Título\nUma palavra errada, e outra frase aqui mesmo.\n";

#[test]
fn a_mark_on_the_page_becomes_a_suggestion_for_the_source() {
    let root = std::env::temp_dir().join(format!("pearl-review-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, BODY).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 1100.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(file)).unwrap(),
    );
    until(&mut h, "the pages", |a| a.page_count() > 0);
    h.run_steps(5);

    // Reviewing: a click on a word marks all of it, and writes nothing yet.
    let word = on_screen(&mut h, "errada");
    h.get_by_label("Marcar as páginas prontas, com sugestões para aceitar ou recusar").click();
    h.run_steps(3);
    assert_eq!(h.state().review_tool(), Some(ReviewTool::Strike));
    click_at(&mut h, word);
    until(&mut h, "the composer", |a| a.composing_mark().is_some());
    assert_eq!(h.state().composing_mark().unwrap(), ("errada".to_string(), ReviewTool::Strike));
    h.run_steps(3);
    h.get_by_label("Marcar").click();
    h.run_steps(3);
    assert_eq!(h.state().text(), BODY, "a mark is only a suggestion");
    assert!(!h.state().is_dirty());
    assert!(h.state().mark_applies(0));
    assert!(h.query_by_label("Aceitar").is_some());

    // Accepted: the word goes, as one undo step, and the mark is over.
    h.get_by_label("Aceitar").click();
    h.run_steps(3);
    assert_eq!(h.state().text(), BODY.replace("errada", ""), "{:?}", h.state().text());
    assert!(h.state().mark_outcome(0).is_some());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| a.text() == BODY);

    // A drag marks what it goes over, with what replaces it. Declined, nothing changes.
    let (from, to) = (on_screen(&mut h, "outra"), on_screen(&mut h, "frase"));
    drag_over(&mut h, from, to);
    until(&mut h, "the composer again", |a| a.composing_mark().is_some());
    assert_eq!(h.state().composing_mark().unwrap().0, "outra frase");
    h.run_steps(3);
    h.state_mut().finish_mark("uma ideia");
    h.run_steps(3);
    let index = h.state().comment_threads().len() - 1;
    assert!(h.query_by_label("Trocar por: uma ideia").is_some());
    h.state_mut().decline_mark(index);
    h.run_steps(3);
    assert_eq!(h.state().text(), BODY);
    assert!(!h.state().mark_applies(index));

    // Another, and the text under it changes first: it can no longer be applied, and says so.
    let word = on_screen(&mut h, "palavra");
    click_at(&mut h, word);
    until(&mut h, "the third composer", |a| a.composing_mark().is_some());
    h.state_mut().finish_mark("");
    h.run_steps(3);
    let index = h.state().comment_threads().len() - 1;
    assert!(h.state().mark_applies(index));
    let changed = BODY.replace("palavra", "palavrinha");
    h.state_mut().set_text(&changed);
    h.run_steps(5);
    assert!(!h.state().mark_applies(index));
    assert!(h.query_by_label("O texto mudou desde esta marca, e ela não pode mais ser aplicada. Dá para recusá-la ou refazê-la.").is_some());
    let ctx = h.ctx.clone();
    h.state_mut().accept_mark(&ctx, index);
    h.run_steps(3);
    assert_eq!(h.state().text(), changed, "never applied blindly");

    // On what the document generates, it says so.
    h.state_mut().set_review_tool(None);
    h.state_mut().set_text("#set page(margin: 40pt)\n#lorem(30)\n");
    let started = h.state().comment_threads().len();
    assert!(started > 0);
    until(&mut h, "the new pages", |a| a.error_count() == 0);
    h.run_steps(30);
    h.state_mut().set_review_tool(Some(ReviewTool::Highlight));
    h.state_mut().pick_on_page(0, [120.0, 45.0], [120.0, 45.0]);
    until(&mut h, "the refusal", |a| a.review_refusal().is_some());
    assert_eq!(h.state().review_refusal(), Some(PickRefusal::Generated));
    h.run_steps(3);
    assert!(
        h.query_by_label(
            "Isto vem do código do documento, como um número ou uma referência, e não tem texto próprio para marcar. Comente no código."
        )
        .is_some()
    );
    assert!(h.state().composing_mark().is_none());

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
