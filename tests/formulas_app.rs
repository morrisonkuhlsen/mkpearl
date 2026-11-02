//! Ready-made formulas through the window: found, previewed and put in the document, which compiles.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

#[test]
fn a_formula_is_found_and_goes_into_the_document() {
    let root = std::env::temp_dir().join(format!("pearl-formulas-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let mut app = App::new(None).unwrap();
    app.set_text("Início\n");
    app.set_tab("math");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(3);
    // Both ways there are named: the Math tab's button, and the strip's item, which opens on them.
    assert!(h.query_by_label("Fórmula quadrática").is_none());
    h.get_by_label("Fórmulas prontas").click();
    h.run_steps(3);
    assert!(h.query_by_label("Estatística descritiva").is_some(), "every subject shows at once");
    h.get_by_label("Álgebra").click();
    h.run_steps(3);
    assert!(h.query_by_label("Fórmula quadrática").is_some());
    h.state_mut().open_side(usize::MAX);
    h.run_steps(3);
    assert!(h.query_by_label("Fórmula quadrática").is_none());
    h.get_by_label("Fórmulas").click();
    h.run_steps(3);
    assert!(h.query_by_label("Fórmula quadrática").is_some(), "and opens it on the formulas");
    // The symbols are one click away, under names people can read.
    h.get_all_by_label("Símbolos").last().unwrap().click();
    h.run_steps(3);
    assert!(h.query_by_label("Letras gregas").is_some(), "not the message's id");
    h.state_mut().open_formulas();
    h.run_steps(3);
    assert!(h.query_by_label("Teorema de Bayes").is_none(), "the others are closed");

    // A search reaches into the closed subjects, without accents.
    h.state_mut().search_formulas("desvio padrao");
    h.run_steps(3);
    h.get_by_label("Desvio padrão amostral").click();
    h.run_steps(3);
    assert!(h.state().text().contains("$ s = sqrt(1 / (n - 1)"), "{:?}", h.state().text());

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().page_count() > 0 && h.state().error_count() == 0, "{}", h.state().error_count());
    h.state_mut().search_formulas("xyzzy");
    h.run_steps(3);
    assert!(h.query_by_label_contains("Nada encontrado").is_some());

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
