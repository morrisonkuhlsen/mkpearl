//! Editing on the pages, with the pointer and the keyboard, through the window: the source is the only
//! thing that changes, and the pages follow it.

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use pearl::app::App;
use pearl::ruler::Edge;

fn window<'a>(app: App) -> Harness<'a, App> {
    Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?}, caret {:?}", h.state().text(), h.state().page_caret());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn click(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.event(Event::PointerMoved(at));
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
}

fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
    h.event(Event::PointerMoved(from));
    h.step();
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    let middle = from + (to - from) * 0.5;
    h.event(Event::PointerMoved(middle));
    h.step();
    h.event(Event::PointerMoved(to));
    h.step();
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    h.step();
}

#[test]
fn text_is_typed_on_the_page_and_margins_are_dragged() {
    let root = std::env::temp_dir().join(format!("pearl-page-editing-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let file = root.join("doc.typ");
    const TEXT: &str = "#set page(margin: 2cm)\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n";
    std::fs::write(&file, TEXT).unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.show_pages_only();
    let mut h = window(app);
    until(&mut h, "the page", |a| a.page_rect(0).is_some() && a.page_count() > 0);
    h.step();

    // A click on the text puts the caret there, with only the pages on show.
    let page = h.state().page_rect(0).unwrap();
    let zoom = page.width() / 595.28;
    let two_cm = 2.0 * 72.0 / 2.54;
    // The fourth letter of the line, near the top of the writing area.
    let target = page.min + egui::vec2(two_cm + 0.0, two_cm + 6.0) * zoom;
    let target = target + egui::vec2(3.5 * 7.2 * zoom, 0.0);
    click(&mut h, target);
    until(&mut h, "the caret on the page", |a| a.page_caret().is_some());
    let (on, x, y, size) = h.state().page_caret().unwrap();
    assert_eq!(on, 0);
    assert!(x > two_cm && x < two_cm + 60.0, "x {x}");
    assert!(y > two_cm && y < two_cm + 20.0, "y {y}");
    assert!(size > 8.0, "size {size}");

    // What's typed goes into the source at that place, and the page and its caret follow.
    h.event(Event::Text("Z".to_string()));
    until(&mut h, "the letter", |a| a.text().contains('Z'));
    let typed = h.state().text().to_string();
    let line = typed.lines().nth(1).unwrap();
    let at = line.find('Z').unwrap();
    assert!(at > 0 && at < 10, "{line}");
    assert_eq!(typed.len(), TEXT.len() + 1);
    let before = h.state().page_caret().unwrap().1;
    until(&mut h, "the caret to move", |a| a.page_caret().is_some_and(|c| c.1 > before));

    // Deleting works too, and undo takes the typing back.
    h.key_press(Key::Backspace);
    until(&mut h, "the deletion", |a| !a.text().contains('Z'));
    assert_eq!(h.state().text(), TEXT);
    h.event(Event::Text("Y".to_string()));
    until(&mut h, "another letter", |a| a.text().contains('Y'));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| !a.text().contains('Y'));

    // A click in the blank below the text goes to the nearest text, ready for typing.
    // The page is larger than the window: this is still in view.
    let bottom = page.min + egui::vec2(page.width() * 0.5, 400.0);
    click(&mut h, bottom);
    let line_end = h.state().page_caret().map(|c| c.1);
    let waited = Instant::now();
    while waited.elapsed() < Duration::from_millis(500) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    let moved_to = h.state().page_caret();
    assert_ne!(moved_to.map(|c| c.1), line_end, "the caret went to the end of the text");
    h.event(Event::Text("Q".to_string()));
    until(&mut h, "typing after a click in the blank", |a| a.text().contains('Q'));
    assert!(h.state().text().lines().nth(1).unwrap().contains('Q'), "{:?} {moved_to:?}", h.state().text());
    h.key_press(Key::Backspace);
    until(&mut h, "the deletion", |a| !a.text().contains('Q'));

    // Dragging a margin on the ruler changes only the rule in the source, as one undo step.
    until(&mut h, "the margins", |a| a.margin_handle(Edge::Left).is_some());
    let text_before = h.state().text().to_string();
    let handle = h.state().margin_handle(Edge::Left).unwrap();
    let to = handle + egui::vec2(two_cm * 0.5 * zoom, 0.0);
    drag(&mut h, handle, to);
    until(&mut h, "the rule", |a| a.text().contains("left: 3cm"));
    let dragged = h.state().text().to_string();
    assert!(dragged.starts_with("#set page(margin: (top: 2cm, bottom: 2cm, left: 3cm, right: 2cm))\n"), "{dragged}");
    assert_eq!(dragged.lines().count(), text_before.lines().count());
    assert!(dragged.contains(text_before.lines().nth(1).unwrap()), "{dragged}");
    // The pages and the rulers take the new margin.
    let moved = h.state().margin_handle(Edge::Left);
    let started = Instant::now();
    let mut latest = moved;
    while latest.is_none_or(|m| (m.x - to.x).abs() > 3.0) {
        assert!(started.elapsed() < Duration::from_secs(20), "{latest:?} / {to:?}");
        h.step();
        std::thread::sleep(Duration::from_millis(5));
        latest = h.state().margin_handle(Edge::Left);
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the margin's undo", |a| a.text() == text_before);

    // The vertical rulers drag the same way.
    until(&mut h, "the old margin", |a| a.margin_handle(Edge::Top).is_some());
    let top = h.state().margin_handle(Edge::Top).unwrap();
    drag(&mut h, top, top + egui::vec2(0.0, two_cm * 0.5 * zoom));
    until(&mut h, "the top margin", |a| a.text().contains("top: 3cm"));

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
