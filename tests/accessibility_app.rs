//! Accessibility through the window: found and listed, a warning before exporting that never stops it,
//! and an accessible PDF that says what's missing or gets written.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {} / {:?}", h.state().status(), h.state().export_refusal());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

const DOC: &str = "= Um\n=== Três\nTexto.\n";
const FIXED: &str = "#set document(title: [Um])\n#set text(lang: \"pt\")\n= Um\n== Dois\nTexto.\n";

#[test]
fn exporting_warns_first_and_an_accessible_pdf_says_what_is_missing() {
    let root = std::env::temp_dir().join(format!("pearl-accessibility-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let file = root.join("doc.typ");
    let pdf = root.join("doc.pdf");
    std::fs::write(&file, DOC).unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.set_export_target(pdf.clone());
    app.open_side(4);
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the finding", |a| a.typography_findings().iter().any(|f| f.kind.is_accessibility()));
    h.run_steps(3);
    assert!(h.query_by_label("Acessibilidade").is_some(), "its own part of the pane");

    // The warning comes first, and cancelling writes nothing.
    h.state_mut().export_pdf();
    h.run_steps(3);
    assert!(h.state().export_warning_open());
    h.get_by_label("Cancelar").click();
    h.run_steps(3);
    assert!(!h.state().export_warning_open() && !pdf.exists());

    // Passing it exports all the same: it never stops the export.
    h.state_mut().export_pdf();
    h.run_steps(3);
    h.get_by_label("Exportar mesmo assim").click();
    until(&mut h, "the PDF", |_| pdf.is_file());
    std::fs::remove_file(&pdf).unwrap();

    // Turned off, it stays off, and the export just happens.
    h.state_mut().export_pdf();
    h.run_steps(3);
    h.get_by_label("Não avisar de novo antes de exportar").click();
    h.run_steps(2);
    h.get_by_label("Cancelar").click();
    h.run_steps(3);
    h.state_mut().export_pdf();
    h.run_steps(3);
    assert!(!h.state().export_warning_open());
    until(&mut h, "the PDF without a warning", |_| pdf.is_file());
    std::fs::remove_file(&pdf).unwrap();

    // An accessible PDF that can't be: plain reasons, in the language on show, and no file.
    h.state_mut().export_accessible_pdf();
    until(&mut h, "the refusal", |a| a.export_refusal().is_some());
    h.run_steps(3);
    let reasons = h.state().export_refusal().unwrap();
    assert!(reasons.iter().any(|r| r.contains("pula um nível")), "{reasons:?}");
    assert!(!pdf.exists());
    h.get_by_label("Fechar").click();
    h.run_steps(3);
    assert!(h.state().export_refusal().is_none());

    // Typst says one thing at a time: with that out of the way, what's next.
    h.state_mut().set_text("= Um\n== Dois\nTexto.\n");
    until(&mut h, "the heading fixed", |a| a.typography_findings().is_empty());
    h.state_mut().export_accessible_pdf();
    until(&mut h, "the next refusal", |a| a.export_refusal().is_some());
    h.run_steps(3);
    let reasons = h.state().export_refusal().unwrap();
    assert!(reasons.iter().any(|r| r.contains("não tem título")), "{reasons:?}");
    assert!(!pdf.exists());
    h.get_by_label("Fechar").click();
    h.run_steps(3);

    // With what was missing, it's written and nothing is found.
    h.state_mut().set_text(FIXED);
    until(&mut h, "nothing to point out", |a| a.typography_findings().is_empty());
    h.state_mut().export_accessible_pdf();
    until(&mut h, "the accessible PDF", |_| pdf.is_file());
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
