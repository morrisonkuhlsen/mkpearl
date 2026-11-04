//! Saving through the window: the file holds the old text or the new one, never part of either.
#![cfg(unix)]

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} {:?}", h.state().text(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn autosave_replaces_the_file_whole_and_says_when_it_cant() {
    let root = std::env::temp_dir().join(format!("pearl-saving-{}", std::process::id()));
    let folder = root.join("docs");
    std::fs::create_dir_all(&folder).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = folder.join("doc.typ");
    std::fs::write(&file, "antes\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    let before = std::fs::metadata(&file).unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(file.clone())).unwrap(),
    );
    h.run_steps(2);

    // A new file took the old one's place all at once, with its permissions, and nothing beside it.
    h.event(egui::Event::Text("primeira ".to_string()));
    wait(&mut h, "the typing", |a| a.text().contains("primeira"));
    let started = Instant::now();
    while !std::fs::read_to_string(&file).unwrap().contains("primeira") {
        assert!(started.elapsed() < Duration::from_secs(15), "autosave wrote the file");
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = std::fs::metadata(&file).unwrap();
    assert_ne!(after.ino(), before.ino(), "the file was written over in place");
    assert_eq!(after.mode() & 0o7777, 0o640);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "only the document, without temporary files");
    let saved = std::fs::read_to_string(&file).unwrap();

    // Nowhere to write: the file stays as it was, the text stays unsaved, and the status says so.
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    wait(&mut h, "the clean state", |a| !a.is_dirty());
    h.event(egui::Event::Text("segunda ".to_string()));
    wait(&mut h, "the typing", |a| a.text().contains("segunda"));
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = h.state().status().to_string();
    let dirty = h.state().is_dirty();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(dirty, "unsaved text isn't marked as saved");
    assert!(status.contains("salvar") || status.contains("save"), "{status:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
