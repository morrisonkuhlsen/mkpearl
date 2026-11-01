//! The Documentation pane through the window: find a lesson, change its example, see the result and
//! bring it into the document; look a function up; F1 at the caret.

use std::time::{Duration, Instant};

use egui::Modifiers;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pearl::app::App;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&mut Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?}", h.state().text());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn lessons_are_found_tried_and_brought_into_the_document() {
    let root = std::env::temp_dir().join(format!("pearl-docs-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let mut app = App::new(None).unwrap();
    app.set_text("Início\n");
    app.open_side(8);
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(3);

    // The home page says how far you've got, and offers the first lesson.
    assert!(h.query_by_label_contains("Lições lidas: 0 de").is_some());
    h.get_by_label_contains("Continuar: Seu primeiro documento").click();
    h.run_steps(3);
    assert!(h.query_by_label("Seu primeiro documento").is_some());
    assert!(h.query_by_label_contains("Tente: troque").is_some());

    // Its example runs by itself, and an edit runs again.
    until(&mut h, "the example", |h| h.query_by_label("Executando…").is_none());
    assert!(h.query_by_label("Restaurar").is_none());
    h.get_all_by_role(egui::accesskit::Role::MultilineTextInput)
        .find(|node| node.accesskit_node().value().is_some_and(|value| value.contains("Olá, mundo")))
        .unwrap()
        .click();
    h.run_steps(2);
    // A mistake says what's wrong, right there, and the example comes back with a click.
    h.event(egui::Event::Text("#nao_existe()".to_string()));
    until(&mut h, "the example's error", |h| h.query_by_label_contains("nao_existe").is_some());
    assert_eq!(h.state().text(), "Início\n", "the document isn't touched");
    h.get_by_label("Restaurar").click();
    until(&mut h, "the example restored", |h| h.query_by_label("Restaurar").is_none() && h.query_by_label_contains("nao_existe").is_none());

    // Reading it counts, and the next lesson is a click away.
    h.get_by_label("Próxima").click();
    h.run_steps(3);
    assert!(h.query_by_label("Negrito, itálico e sublinhado").is_some());
    let saved = std::fs::read_to_string(root.join("config/pearl/guide.txt")).unwrap();
    assert_eq!(saved.lines().collect::<Vec<_>>(), ["0", "1"]);

    // An example goes into the document at the caret, as one more edit.
    until(&mut h, "the second example", |h| h.query_by_label("Executando…").is_none());
    h.get_all_by_label("Inserir no cursor").next().unwrap().click();
    h.run_steps(3);
    assert!(h.state().text().contains("*importante*"), "{:?}", h.state().text());

    // A search finds lessons without accents, and functions.
    h.state_mut().search_docs("matematica");
    h.run_steps(3);
    assert!(h.query_by_label("Fórmulas").is_some());
    h.state_mut().search_docs("xyzzyqqq");
    h.run_steps(3);
    assert!(h.query_by_label_contains("Nada encontrado").is_some());
    h.state_mut().search_docs("table");
    h.run_steps(3);
    h.get_by_label("table").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("Vem do próprio Typst").is_some());
    assert_eq!(h.state().docs_search(), "", "opening a result leaves the search");

    // F1 looks up the word at the caret.
    h.state_mut().set_text("#table");
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::NONE, egui::Key::F1);
    h.run_steps(2);
    assert_eq!(h.state().docs_search(), "table");

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
