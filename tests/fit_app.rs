//! Fitting in pages through the window: nothing changes until the difference is accepted, the pages
//! promised are the pages the document gets, one undo goes back, and a target out of reach says how close.

use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::compiler::FitAnswer;
use pearl::fit::Ending;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(90), "{what}: pages {} / {}", h.state().page_count(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn searched(h: &mut Harness<'_, App>) -> FitAnswer {
    until(h, "the search", |a| a.fit_state().is_some_and(|(searching, answer)| !searching && answer.is_some()));
    h.run_steps(3);
    h.state().fit_state().unwrap().1.unwrap().unwrap()
}

#[test]
fn a_document_is_fitted_and_the_writer_decides() {
    let root = std::env::temp_dir().join(format!("pearl-fit-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let file = root.join("doc.typ");
    let paragraph = "Um texto corrido que ocupa várias linhas nas páginas. ".repeat(6);
    let body = format!("#set text(lang: \"pt\", size: 11pt)\n{}", format!("{paragraph}\n\n").repeat(30));
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
    let pages = h.state().page_count();
    assert!(pages >= 3, "{pages}");

    // A target within reach: found, shown, and nothing written yet.
    h.get_by_label("Encontrar os menores ajustes de layout para o documento caber em um número de páginas").click();
    h.run_steps(3);
    h.get_by_label("Procurar").click();
    h.run_steps(2);
    let answer = searched(&mut h);
    assert_eq!(answer.ending, Ending::Reached);
    assert!(answer.pages_after < pages);
    assert_eq!(h.state().text(), body, "looking changes nothing");
    assert!(h.query_all_by_label_contains("→").count() > 0, "the difference is on show");

    // Text typed since makes the answer stale: it can't be applied blindly.
    let typed = format!("{body}Mais.\n");
    h.state_mut().set_text(&typed);
    h.run_steps(3);
    assert!(h.query_by_label("Aplicar").is_none());
    assert!(h.query_by_label("O texto mudou desde então. Procure de novo para aplicar.").is_some());
    h.state_mut().set_text(&body);
    h.run_steps(3);

    // Applying writes the rules, as one undo step, and the document gets the pages promised.
    h.get_by_label("Aplicar").click();
    h.run_steps(3);
    assert!(h.state().fit_state().is_none(), "the window closes");
    let written = h.state().text().to_string();
    assert_ne!(written, body);
    assert!(written.ends_with(&format!("{paragraph}\n\n")), "the words stay");
    until(&mut h, "the pages promised", |a| a.page_count() == answer.pages_after && a.error_count() == 0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| a.text() == body);
    until(&mut h, "the pages again", |a| a.page_count() == pages);

    // Out of reach: it says how close, and applying is a choice.
    h.state_mut().open_fit_pages();
    h.state_mut().set_fit_target(1);
    h.state_mut().search_fit_pages();
    let answer = searched(&mut h);
    assert_eq!(answer.ending, Ending::OutOfReach);
    assert!(h.query_by_label("Aplicar mesmo assim").is_some());

    // Fechar leaves the document alone.
    h.get_by_label("Fechar").click();
    h.run_steps(3);
    assert!(h.state().fit_state().is_none());
    assert_eq!(h.state().text(), body);

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
