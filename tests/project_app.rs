//! A folder opened as a project: its main document opens, paths start at its top, switching
//! documents loses no text, and what other programs add or change shows up.

use std::path::Path;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{what}: {:?} {:?} {:?}",
            h.state().document_path(),
            h.state().status(),
            h.state().text()
        );
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Waits for a compile after this moment to come back.
fn compiled(h: &mut Harness<'_, App>, what: &str) {
    let before = h.state().compile_count();
    wait(h, what, |h| h.state().compile_count() > before);
}

fn is(h: &Harness<'_, App>, file: &Path) -> bool {
    h.state().document_path() == Some(file)
}

/// A keystroke in the editor, so the document has unsaved text.
fn type_something(h: &mut Harness<'_, App>) {
    h.event(egui::Event::Text("!".to_string()));
    h.run_steps(2);
    assert!(h.state().is_dirty());
}

fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

#[test]
fn a_folder_opens_as_a_project() {
    let root = std::env::temp_dir().join(format!("pearl-project-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("livro");
    std::fs::create_dir_all(project.join("cap")).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    std::fs::write(project.join("comum.typ"), "#let nome = [Comum]\n").unwrap();
    std::fs::write(project.join("main.typ"), "#import \"comum.typ\": nome\n= Livro\n#nome\n").unwrap();
    // A parent of the chapter's folder, inside the project: only the project's root reaches it.
    std::fs::write(project.join("cap/um.typ"), "#import \"../comum.typ\": nome\n= Um\n#nome\n").unwrap();
    std::fs::write(root.join("fora.typ"), "= Fora\n").unwrap();

    // Given on the command line, the folder opens with its main document.
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(project.clone())).unwrap(),
    );
    let main = project.join("main.typ");
    let chapter = project.join("cap/um.typ");
    wait(&mut h, "the main document", |h| is(h, &main));
    assert_eq!(h.state().project_folder(), Some(project.as_path()));
    assert!(h.state().project_open());
    compiled(&mut h, "the main document's compile");
    assert_eq!(h.state().error_count(), 0, "{:?}", h.state().status());

    // A document from the tree reads a file above its own folder, inside the project.
    click(&mut h, "cap");
    click(&mut h, "um.typ");
    wait(&mut h, "the chapter", |h| is(h, &chapter));
    compiled(&mut h, "the chapter's compile");
    assert_eq!(h.state().error_count(), 0);

    // Paths that leave the project are still refused.
    h.state_mut().set_text("#include \"../../fora.typ\"\n");
    compiled(&mut h, "the compile reaching outside");
    assert!(h.state().error_count() > 0, "a file outside the project was read");

    // Switching documents saves the one being left.
    h.state_mut().set_text("#import \"../comum.typ\": nome\n= Um, editado\n");
    type_something(&mut h);
    click(&mut h, "main.typ");
    wait(&mut h, "back to the main document", |h| is(h, &main));
    assert!(std::fs::read_to_string(&chapter).unwrap().contains("Um, editado"), "what was typed was saved");

    // When it can't be saved, the document stays, and nothing is lost.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        h.state_mut().set_text("= Livro, editado\n");
        type_something(&mut h);
        let typed = h.state().text().to_string();
        let mode = std::fs::metadata(&project).unwrap().permissions();
        std::fs::set_permissions(&main, std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o555)).unwrap();
        click(&mut h, "um.typ");
        h.run_steps(5);
        std::fs::set_permissions(&project, mode).unwrap();
        std::fs::set_permissions(&main, std::fs::Permissions::from_mode(0o644)).unwrap();
        // Root may write anywhere: only an unwritable folder proves this.
        if std::fs::read_to_string(&main).unwrap() != typed {
            assert!(is(&h, &main), "the switch waited");
            assert_eq!(h.state().text(), typed);
            assert!(h.state().is_dirty());
        }
    }

    // A file another program adds shows in the tree, and a change to an imported one compiles again.
    std::fs::write(project.join("novo.typ"), "= Novo\n").unwrap();
    wait(&mut h, "the new file in the tree", |h| h.state().project_entries().iter().any(|p| p == Path::new("novo.typ")));
    let before = h.state().compile_count();
    std::fs::write(project.join("comum.typ"), "#let nome = [Mudou]\n").unwrap();
    wait(&mut h, "the compile after the imported file changed", |h| h.state().compile_count() > before);

    // A folder that's gone says so.
    let ctx = h.ctx.clone();
    h.state_mut().open_folder(&ctx, root.join("sumiu"));
    assert!(h.state().status().contains("sumiu"));
    assert_eq!(h.state().project_folder(), Some(project.as_path()));

    // Closing it leaves the document open, its paths from its own folder again.
    let ctx = h.ctx.clone();
    h.state_mut().close_folder(&ctx);
    assert_eq!(h.state().project_folder(), None);
    assert!(h.state().document_path().is_some());

    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
