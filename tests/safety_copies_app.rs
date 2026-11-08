//! When a safety copy can't be written, the work doesn't carry on as if it had been (#46): a restore
//! stops before replacing the text, and the recovery copies keep being tried, saying meanwhile that
//! the text isn't protected.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;

fn window(app: App) -> Harness<'static, App> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} {:?}", h.state().text(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Makes a folder read-only, or tells that it can't be: whoever may write anywhere, as root may.
fn lock(dir: &Path) -> bool {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let probe = dir.join("probe");
    let locked = std::fs::write(&probe, "").is_err();
    let _ = std::fs::remove_file(probe);
    locked
}

fn unlock(dir: &Path) {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
}

/// Gives a folder back its writing however the test ends, so a failure leaves nothing locked.
struct Unlocks(PathBuf);

impl Drop for Unlocks {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
    }
}

/// The folder under `root` that holds a document's versions, which names the document it keeps.
fn history_folder(root: &Path) -> PathBuf {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|n| n == "document.txt") {
                return dir;
            }
        }
    }
    panic!("no versions under {}", root.display());
}

#[test]
fn failed_safety_copies_stop_the_restore_and_keep_recovery_trying() {
    let root = std::env::temp_dir().join(format!("pearl-safety-copies-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }

    // The recovery copy: one failure doesn't turn it off.
    let mut app = App::new(None).unwrap();
    app.enable_recovery();
    let mut h = window(app);
    wait(&mut h, "the first compile", |a| a.page_count() == 1);
    h.event(egui::Event::Text("sem salvar".to_string()));
    wait(&mut h, "the first copy", |a| a.has_recovery_copy());
    let recovery = root.join("data").join("pearl").join("recovery");
    let _unlocks = Unlocks(recovery.clone());
    if !lock(&recovery) {
        unlock(&recovery);
        eprintln!("skipped: this user can write in a read-only folder");
        return;
    }
    h.event(egui::Event::Text(", mais".to_string()));
    wait(&mut h, "the failure, said", |a| a.recovery_failure().is_some());
    assert!(h.state().status().contains("tentando de novo"), "{}", h.state().status());
    unlock(&recovery);
    wait(&mut h, "the copies again", |a| a.recovery_failure().is_none());
    assert!(h.state().status().contains("voltou"), "{}", h.state().status());
    let copied = std::fs::read_dir(&recovery)
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .any(|text| text.contains("sem salvar, mais"));
    assert!(copied, "the latest text is in the copy");
    drop(h);

    // The restore: if what it would replace can't be kept, nothing is replaced.
    let file = root.join("doc.typ");
    std::fs::write(&file, "original\n").unwrap();
    let mut h = window(App::new(Some(file.clone())).unwrap());
    wait(&mut h, "the first compile", |a| a.page_count() == 1);
    h.event(egui::Event::Text("mudado ".to_string()));
    wait(&mut h, "the edit", |a| a.text().contains("mudado"));
    wait(&mut h, "the autosave's version", |a| a.version_labels().len() >= 2);
    let versions = history_folder(&root.join("data"));
    let oldest = h.state().version_labels().len() - 1;
    h.state_mut().view_version(oldest);
    // Text the history doesn't have yet, which the restore would replace.
    h.state_mut().set_text("ainda não guardado\n");
    // The history keeps its folder its owner's alone, so a read-only one wouldn't stay so: a file
    // where the folder should be makes the write fail as a full disk would.
    let aside = versions.with_extension("aside");
    std::fs::rename(&versions, &aside).unwrap();
    std::fs::write(&versions, "").unwrap();
    let ctx = h.ctx.clone();
    h.state_mut().restore_version(&ctx);
    assert_eq!(h.state().text(), "ainda não guardado\n", "the text stays");
    assert!(h.state().status().contains("Nada foi restaurado"), "{}", h.state().status());
    // Once the history can be written, the same restore goes ahead, with what it replaced kept.
    std::fs::remove_file(&versions).unwrap();
    std::fs::rename(&aside, &versions).unwrap();
    h.state_mut().restore_version(&ctx);
    assert_eq!(h.state().text(), "original\n");
    assert_eq!(h.state().version_labels().first().copied(), Some("Antes de restaurar"));
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
