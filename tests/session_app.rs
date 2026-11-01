//! Two whole windows in one session, over a real connection: what each shows, compiles and keeps.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;

fn window<'a>(app: App) -> Harness<'a, App> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_max_steps(u64::MAX)
        .with_accessibility_check(false)
        .build_ui_state(
            |ui, app: &mut App| {
                ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                app.show(ui);
            },
            app,
        )
}

fn until(a: &mut Harness<'_, App>, b: &mut Harness<'_, App>, what: &str, done: impl Fn(&App, &App) -> bool) {
    let started = Instant::now();
    while !done(a.state(), b.state()) {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{what}: host {:?} errors {} / guest {:?} errors {}",
            a.state().text(),
            a.state().error_count(),
            b.state().text(),
            b.state().error_count()
        );
        a.step();
        b.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn two_windows_edit_one_document_with_a_picture() {
    let root = std::env::temp_dir().join(format!("pearl-e2e-{}", std::process::id()));
    std::fs::create_dir_all(root.join("fig")).unwrap();
    // The test must not touch the user's list of recent documents.
    // SAFETY: this binary has this one test, and it sets this before any thread reads the variable.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    image::RgbImage::new(4, 4).save(root.join("fig/foto.png")).unwrap();
    let file = root.join("doc.typ");
    std::fs::write(&file, "= Título\n#image(\"fig/foto.png\", width: 1cm)\n").unwrap();

    let mut host = App::new(Some(file.clone())).unwrap();
    let link = host.host_session_on("127.0.0.1:0", "Ana").unwrap();
    let mut a = window(host);
    let mut b = window(App::new(None).unwrap());
    let ctx = b.ctx.clone();
    b.state_mut().join_session_link(&ctx, &link, "Beto");

    // The guest gets the text, and the picture it needs: it compiles with no errors.
    until(&mut a, &mut b, "guest compiles the document", |a, b| {
        b.text() == a.text() && b.page_count() > 0 && b.error_count() == 0 && !a.session_others().is_empty()
    });
    assert_eq!(a.state().session_others(), ["Beto"]);

    // Edits on the guest reach the host, its pages, and in a moment its file.
    b.state_mut().set_text("= Título\n#image(\"fig/foto.png\", width: 1cm)\nDo Beto.\n");
    until(&mut a, &mut b, "the edit reaches the host", |a, _| a.text().contains("Do Beto."));
    assert!(a.state().is_dirty(), "the host's file has changes to save");
    let saved = Instant::now();
    while !std::fs::read_to_string(&file).unwrap().contains("Do Beto.") {
        assert!(saved.elapsed() < Duration::from_secs(15), "autosave wrote the guest's edit to the host's file");
        a.step();
        b.step();
        std::thread::sleep(Duration::from_millis(20));
    }

    // Edits on the host reach the guest; both carets are known to the other side.
    a.state_mut().set_text("= Título\n#image(\"fig/foto.png\", width: 1cm)\nDo Beto.\nDa Ana.\n");
    until(&mut a, &mut b, "the edit reaches the guest", |_, b| b.text().contains("Da Ana."));
    until(&mut a, &mut b, "carets are shared", |a, b| !a.session_carets().is_empty() && !b.session_carets().is_empty());
    assert_eq!(b.state().session_carets()[0].name, "Ana");
    assert_eq!(a.state().session_carets()[0].name, "Beto");

    // The host leaves: the guest keeps the text, marked unsaved.
    a.state_mut().leave_session();
    until(&mut a, &mut b, "the guest learns the session ended", |_, b| b.is_dirty() && b.session_carets().is_empty());
    assert!(b.state().text().contains("Da Ana."));
    drop(a);
    drop(b);
    std::fs::remove_dir_all(root).unwrap();
}
