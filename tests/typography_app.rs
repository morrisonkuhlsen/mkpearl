//! The typography watch through the window: found on the pages, listed, followed to its passage, and
//! ignored for as long as the passage stays, also in a later run.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::typography::Kind;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?}", h.state().typography_findings());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn window(app: App) -> Harness<'static, App> {
    Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

const FAR: &str =
    "Veja a @fig:um.\n#pagebreak()\nMais.\n#pagebreak()\nMais.\n#pagebreak()\n#figure(rect(width: 2cm)[Desenho], caption: [Um desenho]) <fig:um>\n";

#[test]
fn what_the_watch_finds_is_listed_followed_and_ignored() {
    let root = std::env::temp_dir().join(format!("pearl-typography-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, FAR).unwrap();
    let mut app = App::new(Some(file.clone())).unwrap();
    app.open_side(4);
    let mut h = window(app);
    until(&mut h, "the finding", |a| a.typography_findings().len() == 1);
    assert_eq!(h.state().typography_findings()[0].kind, Kind::FarFigure { mention: 0 });
    h.run_steps(2);

    // The message, in the window's language, goes to its passage.
    let message = "Esta figura está na página 4, mas é mencionada pela primeira vez na página 1.";
    h.get_by_label(message).click();
    h.run_steps(3);
    let caret = h.state().selected_range(&h.ctx).0;
    assert!(FAR.chars().skip(caret).collect::<String>().starts_with("figure("), "the caret is at {caret}");

    // Ignored, it stays so when the document compiles again, wherever the passage has moved to.
    h.get_by_label("Ignorar").click();
    h.run_steps(3);
    assert!(h.state().typography_findings().is_empty());
    assert!(h.query_by_label(message).is_none());
    h.state_mut().set_text(&format!("Antes.\n\n{FAR}"));
    let compiles = h.state().page_count();
    until(&mut h, "the new compile", |a| a.text().starts_with("Antes") && a.page_count() == compiles);
    h.run_steps(30);
    assert!(h.state().typography_findings().is_empty(), "{:?}", h.state().typography_findings());

    // A later run remembers, and the writer can have it back.
    drop(h);
    std::fs::write(&file, FAR).unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.open_side(4);
    let mut h = window(app);
    until(&mut h, "the first compile", |a| a.page_count() > 0);
    h.run_steps(30);
    assert!(h.state().typography_findings().is_empty());
    h.get_by_label("Mostrar o que foi ignorado (1)").click();
    h.run_steps(3);
    assert_eq!(h.state().typography_findings().len(), 1);

    // Rewriting the passage asks again, since it's another passage.
    h.get_by_label("Ignorar").click();
    h.run_steps(3);
    h.state_mut().set_text(&FAR.replace("Um desenho", "Outro desenho"));
    until(&mut h, "the rewritten passage", |a| a.typography_findings().len() == 1);

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
