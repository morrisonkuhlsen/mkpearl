//! A document's past through the window: what writes over the file stays recoverable.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} {:?}", h.state().text(), h.state().version_labels());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn what_autosave_writes_over_can_be_seen_and_restored() {
    let root = std::env::temp_dir().join(format!("pearl-versions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, "original\n").unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(file.clone())).unwrap(),
    );
    assert_eq!(h.state().version_labels(), ["Ao abrir"], "the file as it was, before anything can touch it");
    wait(&mut h, "the first compile", |a| a.page_count() == 1);

    // An edit: two pages now, written to the file by autosave, with a version of it.
    h.event(egui::Event::Text("mudado\n#pagebreak()\nsegunda".to_string()));
    h.step();
    wait(&mut h, "the edit", |a| a.text().contains("segunda") && a.page_count() == 2);
    let started = Instant::now();
    while !std::fs::read_to_string(&file).unwrap().contains("segunda") {
        assert!(started.elapsed() < Duration::from_secs(15), "autosave wrote the file");
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().version_labels(), ["Salvamento automático", "Ao abrir"]);
    let current = h.state().text().to_string();

    // Looking at the old one: the pages show it, and the document isn't touched.
    h.state_mut().view_version(1);
    h.state_mut().preview_version(true);
    wait(&mut h, "the old version's pages", |a| a.page_count() == 1);
    assert_eq!(h.state().text(), current);
    h.state_mut().preview_version(false);
    wait(&mut h, "the document's pages again", |a| a.page_count() == 2);

    // Restoring: the text returns, reaches the file, and what it replaced is kept too.
    h.state_mut().view_version(1);
    let ctx = h.ctx.clone();
    h.state_mut().restore_version(&ctx);
    assert!(h.state().text().starts_with("original"), "{:?}", h.state().text());
    let started = Instant::now();
    while std::fs::read_to_string(&file).unwrap().contains("segunda") {
        assert!(started.elapsed() < Duration::from_secs(15), "the restored text reached the file");
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    h.state_mut().view_version(0);
    drop(h);
    // Everything that ever was in the file is still there, on disk, under the data folder.
    let mut kept = Vec::new();
    let mut pending = vec![root.join("data")];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            if entry.path().is_dir() {
                pending.push(entry.path());
            } else if entry.path().extension().is_some_and(|e| e == "typ") {
                kept.push(std::fs::read_to_string(entry.path()).unwrap());
            }
        }
    }
    assert!(kept.iter().any(|t| t == "original\n"), "{kept:?}");
    assert!(kept.iter().any(|t| *t == current), "{kept:?}");
    // And the document's folder holds only what it held: `data` and `config` are this test's own.
    let mut beside: Vec<String> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    beside.sort();
    assert_eq!(beside, ["config", "data", "doc.typ"]);
    std::fs::remove_dir_all(root).unwrap();
}
