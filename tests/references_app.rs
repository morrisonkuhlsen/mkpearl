//! The References pane through the window: a `.bib` comes in, is searched, and a click cites it in a
//! document that then compiles, with its bibliography added only when it has none.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

const BIB: &str = r#"
@article{goncalves2020, title = {Sobre as pérolas}, author = {Gonçalves, Maria}, year = {2020}, journal = {Revista} }
@book{knuth84, title = {The TeXbook}, author = {Knuth, Donald}, year = {1984}, publisher = {Addison} }
"#;

fn until(h: &mut Harness<'_, App>, what: &str, mut done: impl FnMut(&mut Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} / {}", h.state().text(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn settled(h: &mut Harness<'_, App>, what: &str) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2500) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().page_count() > 0 && h.state().error_count() == 0, "{what}: {} errors in {:?}", h.state().error_count(), h.state().text());
}

#[test]
fn a_reference_is_imported_found_and_cited() {
    let root = std::env::temp_dir().join(format!("pearl-references-{}", std::process::id()));
    let (project, away) = (root.join("doc"), root.join("away"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&away).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    std::fs::write(away.join("fontes.bib"), BIB).unwrap();
    let file = project.join("doc.typ");
    std::fs::write(&file, "Como disse\n").unwrap();

    let mut app = App::new(Some(file)).unwrap();
    app.open_side(7);
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the empty pane", |h| h.query_by_label_contains("Não há arquivos .bib").is_some());

    let ctx = h.ctx.clone();
    h.state_mut().import_references_from(&ctx, &away.join("fontes.bib"));
    assert!(project.join("fontes.bib").is_file());
    assert!(h.state().status().contains("fontes.bib"), "{}", h.state().status());
    until(&mut h, "the list", |h| h.query_by_label("The TeXbook").is_some());
    assert!(h.query_by_label("Sobre as pérolas").is_some());

    // A search narrows it, whatever the accents.
    h.state_mut().search_references("perolas");
    h.step();
    h.step();
    assert!(h.query_by_label("The TeXbook").is_none());

    // A click cites it, and the document gets the bibliography it needs.
    h.get_by_label("Sobre as pérolas").click();
    h.step();
    h.step();
    assert!(h.state().text().contains("@goncalves2020"), "{:?}", h.state().text());
    assert!(h.state().text().contains("#bibliography(\"fontes.bib\")"), "{:?}", h.state().text());
    assert!(h.state().status().contains("Bibliografia adicionada"), "{}", h.state().status());
    settled(&mut h, "after the first citation");

    // Another citation adds no second bibliography.
    h.state_mut().search_references("knuth");
    until(&mut h, "the other entry", |h| h.query_by_label("The TeXbook").is_some());
    h.get_by_label("The TeXbook").click();
    h.step();
    h.step();
    assert!(h.state().text().contains("@knuth84"));
    assert_eq!(h.state().text().matches("#bibliography(").count(), 1);
    settled(&mut h, "after the second citation");

    // A file edited outside shows up by itself.
    std::fs::write(project.join("fontes.bib"), format!("{BIB}\n@misc{{nova, title = {{Entrada nova}}}}\n")).unwrap();
    h.state_mut().search_references("");
    until(&mut h, "the edited file", |h| h.query_by_label("Entrada nova").is_some());

    // A second file stays out of what can be cited, and says so; a broken one says that too.
    std::fs::write(project.join("outra.bib"), "@misc{fora, title = {Fora da bibliografia}}").unwrap();
    std::fs::write(project.join("ruim.bib"), "@article{").unwrap();
    until(&mut h, "the note", |h| h.query_by_label_contains("Fora da bibliografia do documento").is_some());
    until(&mut h, "the broken file", |h| h.query_by_label_contains("ruim.bib como bibliografia").is_some());
    assert!(h.query_by_label("Fora da bibliografia").is_none());

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
