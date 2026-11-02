//! Figures on the pages, with the pointer and the keys, through the window: selected, resized, moved and
//! removed, and only the source changes.

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use pearl::app::App;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} selected {:?}", h.state().text(), h.state().selected_image());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn press(h: &mut Harness<'_, App>, at: egui::Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}

fn click(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.event(Event::PointerMoved(at));
    h.step();
    press(h, at, true);
    press(h, at, false);
    h.step();
}

fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
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

/// Where the first image is on screen.
fn image_rect(h: &Harness<'_, App>) -> egui::Rect {
    let image = h.state().placed_images()[0];
    let page = h.state().page_rect(image.page).unwrap();
    let zoom = page.width() / 595.28;
    egui::Rect::from_min_size(page.min + egui::vec2(image.x, image.y) * zoom, egui::vec2(image.width, image.height) * zoom)
}

const FIGURE: &str = "#figure(\n  image(\"foto.png\", width: 4cm),\n  caption: [Legenda],\n)\n";

#[test]
fn an_image_is_resized_moved_and_removed_on_the_page() {
    let root = std::env::temp_dir().join(format!("pearl-figures-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    image::RgbImage::new(40, 20).save(root.join("foto.png")).unwrap();
    let file = root.join("doc.typ");
    let text = format!("Primeiro.\n\n{FIGURE}\nSegundo.\n\nTerceiro.\n");
    std::fs::write(&file, &text).unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.show_pages_only();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the image", |a| a.placed_images().len() == 1 && a.page_rect(0).is_some());
    h.step();
    let four_cm = 4.0 * 72.0 / 2.54;
    assert!((h.state().placed_images()[0].width - four_cm).abs() < 0.5);

    // A click selects it, and changes nothing.
    let area = image_rect(&h);
    click(&mut h, area.center());
    assert_eq!(h.state().selected_image(), Some(0));
    assert_eq!(h.state().text(), text);
    // Esc lets go; a click in the blank does too.
    h.key_press(Key::Escape);
    h.step();
    assert_eq!(h.state().selected_image(), None);
    click(&mut h, area.center());
    let page = h.state().page_rect(0).unwrap();
    click(&mut h, page.min + egui::vec2(page.width() * 0.5, 800.0));
    assert_eq!(h.state().selected_image(), None);

    // The corner resizes it: only `width:` changes, as one undo step.
    click(&mut h, area.center());
    let zoom = page.width() / 595.28;
    drag(&mut h, area.max, area.max + egui::vec2(2.0 * 72.0 / 2.54 * zoom, 0.0));
    until(&mut h, "the new width", |a| a.text().contains("width: 6cm"));
    assert_eq!(h.state().text(), text.replace("width: 4cm", "width: 6cm"));
    until(&mut h, "the image to grow", |a| a.placed_images().first().is_some_and(|i| (i.width - 1.5 * four_cm).abs() < 0.5));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| a.text() == text);
    until(&mut h, "the image to shrink", |a| a.placed_images().first().is_some_and(|i| (i.width - four_cm).abs() < 0.5));
    h.step();

    // Dragged below the last paragraph, its block moves there, and the file isn't touched.
    let area = image_rect(&h);
    drag(&mut h, area.center(), area.center() + egui::vec2(0.0, 220.0));
    until(&mut h, "the move", |a| a.text() != text);
    assert_eq!(h.state().text(), format!("Primeiro.\n\n\nSegundo.\n\nTerceiro.\n{FIGURE}"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the move's undo", |a| a.text() == text);
    until(&mut h, "the image back", |a| a.placed_images().len() == 1);
    h.step();

    // Delete takes it out of the text, and leaves the file.
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(300) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    let area = image_rect(&h);
    click(&mut h, area.center());
    assert_eq!(h.state().selected_image(), Some(0));
    h.key_press(Key::Delete);
    until(&mut h, "the removal", |a| !a.text().contains("#figure"));
    assert!(h.state().text().contains("Segundo."));
    assert!(root.join("foto.png").is_file());
    until(&mut h, "no image on the pages", |a| a.placed_images().is_empty());

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
