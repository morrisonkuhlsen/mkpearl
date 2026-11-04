//! Several carets in the window, as in VS Code: Alt+click, Shift+Alt+drag and the keys for lines.

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::i18n::tx;

const TEXT: &str = "um\ndois\ntres\nquatro\n";

fn window(file: PathBuf) -> Harness<'static, App> {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(file)).unwrap(),
    );
    h.run_steps(3);
    h
}

/// A point on a line of the source, `x` points past the text's left edge.
fn on_line(h: &Harness<'_, App>, line: usize, x: f32) -> Pos2 {
    let rect = h.get_by_label(tx("source-2")).rect();
    let row = h.ctx.fonts_mut(|f| f.row_height(&egui::FontId::monospace(13.0)));
    pos2(rect.min.x as f32 + 4.0 + x, rect.min.y as f32 + 8.0 + (line as f32 + 0.5) * row)
}

fn char_width(h: &Harness<'_, App>) -> f32 {
    h.ctx.fonts_mut(|f| f.glyph_width(&egui::FontId::monospace(13.0), '0'))
}

fn hold(h: &mut Harness<'_, App>, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
}

fn press(h: &mut Harness<'_, App>, at: Pos2, modifiers: Modifiers) {
    hold(h, modifiers);
    h.event(Event::PointerMoved(at));
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers });
    h.step();
}

fn release(h: &mut Harness<'_, App>, at: Pos2, modifiers: Modifiers) {
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers });
    h.step();
    hold(h, Modifiers::NONE);
    h.run_steps(2);
}

fn click(h: &mut Harness<'_, App>, at: Pos2, modifiers: Modifiers) {
    press(h, at, modifiers);
    release(h, at, modifiers);
}

fn key(h: &mut Harness<'_, App>, key: Key, modifiers: Modifiers) {
    hold(h, modifiers);
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.step();
    hold(h, Modifiers::NONE);
    h.run_steps(2);
}

fn typed(h: &mut Harness<'_, App>, event: Event) {
    h.event(event);
    h.run_steps(2);
}

fn text(h: &Harness<'_, App>) -> String {
    h.state().text().to_string()
}

#[test]
fn several_carets_as_in_vs_code() {
    let root = std::env::temp_dir().join(format!("pearl-carets-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, TEXT).unwrap();
    let mut h = window(file);
    let ctx = h.ctx.clone();
    let alt = Modifiers::ALT;
    let shift_alt = Modifiers::SHIFT | Modifiers::ALT;

    // Alt+click adds a caret, and what's typed goes in at both.
    let far = 300.0;
    let (first, third) = (on_line(&h, 0, far), on_line(&h, 2, far));
    click(&mut h, first, Modifiers::NONE);
    click(&mut h, third, alt);
    typed(&mut h, Event::Text("X".into()));
    assert_eq!(text(&h), "umX\ndois\ntresX\nquatro\n");

    // Backspace at both, and a composed letter, as a dead key gives, at both.
    key(&mut h, Key::Backspace, Modifiers::NONE);
    assert_eq!(text(&h), TEXT);
    typed(&mut h, Event::Ime(egui::ImeEvent::Preedit { text: "´".into(), active_range_chars: None }));
    assert_eq!(text(&h), TEXT, "what's being composed isn't in the text yet");
    typed(&mut h, Event::Ime(egui::ImeEvent::Commit("é".into())));
    assert_eq!(text(&h), "umé\ndois\ntresé\nquatro\n");
    key(&mut h, Key::Backspace, Modifiers::NONE);

    // Arrows move both; a new line goes in at both.
    key(&mut h, Key::ArrowLeft, Modifiers::NONE);
    typed(&mut h, Event::Text("-".into()));
    assert_eq!(text(&h), "u-m\ndois\ntre-s\nquatro\n");
    key(&mut h, Key::Backspace, Modifiers::NONE);
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(text(&h), TEXT);

    // Copying takes both lines' selections and pasting spreads them back, a line each.
    key(&mut h, Key::ArrowLeft, Modifiers::SHIFT);
    typed(&mut h, Event::Copy);
    typed(&mut h, Event::Paste("1\n2".into()));
    assert_eq!(text(&h), "u1\ndois\ntre2\nquatro\n");

    // A plain click goes back to one caret.
    let at = on_line(&h, 3, far);
    click(&mut h, at, Modifiers::NONE);
    typed(&mut h, Event::Text("!".into()));
    assert_eq!(text(&h), "u1\ndois\ntre2\nquatro!\n");
    h.state_mut().set_text(TEXT);
    h.run_steps(2);

    // Shift+Alt+Down adds a caret below, and Esc leaves only the main one, the last placed.
    let at = on_line(&h, 0, 0.0);
    click(&mut h, at, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, shift_alt);
    key(&mut h, Key::ArrowDown, Modifiers::COMMAND | Modifiers::ALT);
    typed(&mut h, Event::Text("> ".into()));
    assert_eq!(text(&h), "> um\n> dois\n> tres\nquatro\n");
    key(&mut h, Key::Escape, Modifiers::NONE);
    typed(&mut h, Event::Text("#".into()));
    assert_eq!(text(&h), "> um\n> dois\n> #tres\nquatro\n", "typing goes on after Esc");
    h.state_mut().set_text(TEXT);
    h.run_steps(2);

    // Undo goes back whole and to one caret.
    let at = on_line(&h, 0, 0.0);
    click(&mut h, at, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, shift_alt);
    typed(&mut h, Event::Text("Z".into()));
    assert_eq!(text(&h), "Zum\nZdois\ntres\nquatro\n");
    key(&mut h, Key::Z, Modifiers::COMMAND);
    assert_eq!(text(&h), TEXT);
    typed(&mut h, Event::Text("W".into()));
    assert_eq!(text(&h).matches('W').count(), 1, "one caret after undoing: {:?}", text(&h));
    h.state_mut().set_text(TEXT);
    h.run_steps(2);

    // Text changed from elsewhere, as a peer in a session does, ends the extra carets.
    let at = on_line(&h, 0, 0.0);
    click(&mut h, at, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, shift_alt);
    h.state_mut().set_text("outro\ntexto\n");
    h.run_steps(2);
    typed(&mut h, Event::Text("Q".into()));
    assert_eq!(text(&h).matches('Q').count(), 1, "{:?}", text(&h));
    h.state_mut().set_text(TEXT);
    h.run_steps(2);

    // Shift+Alt+drag makes a box, from the caret to the pointer, and typing replaces it on each line.
    let width = char_width(&h);
    let at = on_line(&h, 0, width);
    click(&mut h, at, Modifiers::NONE);
    let to = on_line(&h, 3, width * 2.0);
    let at = on_line(&h, 0, width);
    press(&mut h, at, shift_alt);
    h.event(Event::PointerMoved(to));
    h.step();
    release(&mut h, to, shift_alt);
    typed(&mut h, Event::Text("_".into()));
    assert_eq!(text(&h), "u_\nd_is\nt_es\nq_atro\n");
    h.state_mut().set_text(TEXT);
    h.run_steps(2);

    // Alt+drag adds a caret with a selection, which typing replaces along with the other's place.
    let at = on_line(&h, 0, 0.0);
    click(&mut h, at, Modifiers::NONE);
    let from = on_line(&h, 3, 0.0);
    press(&mut h, from, alt);
    let to = on_line(&h, 3, width * 3.0);
    for _ in 0..3 {
        h.event(Event::PointerMoved(to));
        h.step();
    }
    release(&mut h, to, alt);
    typed(&mut h, Event::Text("=".into()));
    assert_eq!(text(&h), "=um\ndois\ntres\n=tro\n");
    h.state_mut().set_text(TEXT);
    h.run_steps(2);

    // Alt+click on a caret takes it away.
    let (first, third) = (on_line(&h, 0, far), on_line(&h, 2, far));
    click(&mut h, first, Modifiers::NONE);
    click(&mut h, third, alt);
    click(&mut h, third, alt);
    h.run_steps(2);
    typed(&mut h, Event::Text("K".into()));
    assert_eq!(text(&h), "umK\ndois\ntres\nquatro\n");
    drop(ctx);
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
