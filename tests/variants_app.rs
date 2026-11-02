//! Layout variants through the window: nothing changes while they're looked at, choosing one writes its
//! rules as one undo step, and closing leaves the document as it was.

use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(60), "{what}: {:?} / {}", h.state().layout_variants().map(|v| v.len()), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn all_made(app: &App) -> bool {
    app.layout_variants().is_some_and(|tiles| tiles.iter().all(Option::is_some))
}

#[test]
fn variants_are_looked_at_chosen_and_undone() {
    let root = std::env::temp_dir().join(format!("pearl-variants-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let file = root.join("doc.typ");
    let body = format!(
        "#set text(lang: \"pt\", size: 11pt)\n{}",
        format!("{}\n\n", "Um texto corrido que ocupa várias linhas nas páginas. ".repeat(6)).repeat(50)
    );
    std::fs::write(&file, &body).unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.set_tab("page-layout");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the pages", |a| a.page_count() > 0);
    let pages_now = h.state().page_count();

    // The button opens the window, on variants made one by one.
    h.get_by_label("Ver o documento diagramado de outras formas").click();
    h.run_steps(3);
    assert!(h.state().layout_variants().is_some());
    until(&mut h, "the variants", all_made);
    h.run_steps(3);
    let tiles = h.state().layout_variants().unwrap();
    let pages: Vec<usize> = tiles.iter().map(|t| t.as_ref().unwrap().as_ref().unwrap().0).collect();
    assert_eq!(pages[0], pages_now, "the first is the document as it is");
    assert!(pages[1] < pages[0], "compact takes fewer pages: {pages:?}");
    assert_eq!(h.state().text(), body, "looking changes nothing");

    // A page of each, in turn.
    assert_eq!(h.query_all_by_label("Usar este layout").count(), 5);
    h.get_by_label("›").click();
    h.run_steps(3);
    assert!(h.query_by_label("Página 2").is_some());

    // Closing leaves the document, and a late answer finds nobody.
    h.get_by_label("Fechar").click();
    h.run_steps(3);
    assert!(h.state().layout_variants().is_none());
    assert_eq!(h.state().text(), body);

    // Typing with it open makes them again, once the text rests.
    h.state_mut().open_layout_variants();
    until(&mut h, "the variants again", all_made);
    let typed = format!("{body}Mais.\n");
    h.state_mut().set_text(&typed);
    h.run_steps(2);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) && h.state().layout_variants().is_some_and(|t| t.iter().all(Option::is_some)) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    until(&mut h, "the variants of the new text", all_made);

    // Choosing writes the rules into what's there, closes it, and one undo takes them back.
    h.run_steps(3);
    h.get_all_by_label("Usar este layout").next().unwrap().click();
    h.run_steps(3);
    assert!(h.state().layout_variants().is_none());
    let text = h.state().text().to_string();
    assert!(
        text.contains("#set text(lang: \"pt\", size: 10pt)") && text.contains("#set page(margin: 2cm)") && text.contains("#set par(leading: 0.55em)"),
        "{text}"
    );
    assert!(text.ends_with("Mais.\n"));
    until(&mut h, "the new pages", |a| a.page_count() == pages[1] && a.error_count() == 0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| a.text() == typed);

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
