//! Letterhead and watermark through the window: applied, with a real logo, opened again, changed, undone
//! and taken out, and the rest of the document stays.

use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} / {}", h.state().text(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_letterhead_and_a_watermark_are_applied_changed_and_removed() {
    let root = std::env::temp_dir().join(format!("pearl-letterhead-{}", std::process::id()));
    let (doc, away) = (root.join("doc"), root.join("away"));
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::create_dir_all(&away).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    image::RgbImage::new(20, 20).save(away.join("logo.png")).unwrap();
    let file = doc.join("doc.typ");
    const BODY: &str = "= Ofício\nTexto do ofício.\n";
    std::fs::write(&file, BODY).unwrap();
    let mut app = App::new(Some(file)).unwrap();
    app.set_tab("page-layout");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the page", |a| a.page_count() > 0);

    // The button opens the window, with its fields, and nothing has changed yet.
    h.get_by_label("Timbrado e marca d'água").click();
    h.run_steps(3);
    assert!(h.query_by_label("Marca d'água").is_some());
    assert_eq!(h.state().text(), BODY);

    // Applying writes one rule at the top, brings the logo in, and the document compiles with it.
    let logo = away.join("logo.png").to_string_lossy().into_owned();
    h.state_mut().edit_letterhead(|s| {
        s.head_on = true;
        "Empresa \"X\" #1".clone_into(&mut s.name);
        "Rua A, 1".clone_into(&mut s.details);
        "contato@x.com".clone_into(&mut s.footer);
        s.logo = logo;
        s.mark_on = true;
    });
    h.run_steps(2);
    h.get_by_label("Aplicar").click();
    h.run_steps(3);
    assert!(h.query_by_label("Aplicar").is_none(), "the window closes");
    let applied = h.state().text().to_string();
    assert!(applied.ends_with(BODY), "{applied}");
    assert!(applied.contains("image(\"images/logo.png\", height: 1cm)"), "{applied}");
    assert!(applied.contains("RASCUNHO"), "{applied}");
    assert!(doc.join("images/logo.png").is_file());
    until(&mut h, "the logo on the page", |a| a.placed_images().len() == 1 && a.error_count() == 0);
    assert!(h.state().page_count() > 0);

    // It opens again on what the document has, and a change replaces the block.
    h.state_mut().open_letterhead();
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("Remover").count(), 2, "the logo, and the block a document with one can take out");
    h.state_mut().edit_letterhead(|s| {
        assert_eq!(s.name, "Empresa \"X\" #1");
        assert_eq!(s.logo, "images/logo.png");
        "CÓPIA".clone_into(&mut s.mark_text);
    });
    h.run_steps(2);
    h.get_by_label("Aplicar").click();
    h.run_steps(3);
    assert_eq!(h.state().text().matches("// pearl:letterhead").count(), 1);
    assert!(h.state().text().contains("CÓPIA") && !h.state().text().contains("RASCUNHO"));

    // One undo goes back one step.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| a.text() == applied);

    // A picture as the watermark, faded: the document compiles with it, whatever else it has.
    h.state_mut().open_letterhead();
    h.run_steps(3);
    h.state_mut().edit_letterhead(|s| {
        s.mark_is_image = true;
        s.mark_image = away.join("logo.png").to_string_lossy().into_owned();
        s.mark_opacity = 30.0;
    });
    h.run_steps(2);
    assert!(h.query_all_by_label("Opacidade").count() > 0, "the slider shows for a picture");
    h.get_by_label("Aplicar").click();
    h.run_steps(3);
    assert!(h.state().text().contains("paper.transparentize(30%)"), "{}", h.state().text());
    until(&mut h, "the faded picture on the page", |a| a.placed_images().len() == 2 && a.error_count() == 0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo of the picture", |a| a.text() == applied);

    // Remover leaves the document as it began.
    h.state_mut().open_letterhead();
    h.run_steps(3);
    let remove = h.get_all_by_label("Remover").last().unwrap();
    remove.click();
    h.run_steps(3);
    assert_eq!(h.state().text(), BODY);
    h.key_press(Key::Escape);

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
