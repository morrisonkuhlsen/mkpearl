//! The open file written by another program, as a coding agent in a terminal does: the window
//! follows it by itself, keeps what it had, and never overwrites unsaved text without asking.

use std::path::Path;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::history::Store;
use pearl::i18n::tx;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} {:?}", h.state().text(), h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Steps for a while of real time, for what must not happen.
fn linger(h: &mut Harness<'_, App>) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn asking(h: &Harness<'_, App>) -> bool {
    h.query_by_label(tx("use-the-files-text")).is_some()
}

fn versions(file: &Path) -> Vec<String> {
    let store = Store::for_document(file).unwrap();
    store.list().iter().filter_map(|v| store.read(v)).collect()
}

/// Writes as an agent does, with a time apart from the last write.
fn write_outside(file: &Path, text: &[u8]) {
    let before = std::fs::metadata(file).ok().and_then(|m| m.modified().ok());
    std::fs::write(file, text).unwrap();
    if let Some(before) = before {
        let file = std::fs::File::options().write(true).open(file).unwrap();
        file.set_modified(before + Duration::from_secs(2)).unwrap();
    }
}

#[test]
fn the_window_follows_the_file() {
    let root = std::env::temp_dir().join(format!("pearl-following-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, "antes\n").unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(Some(file.clone())).unwrap(),
    );
    h.run_steps(2);
    std::thread::sleep(Duration::from_millis(300));

    // With nothing unsaved, what another program wrote comes in by itself, and both texts are kept.
    write_outside(&file, "de fora\n".as_bytes());
    wait(&mut h, "the new text", |h| h.state().text() == "de fora\n");
    assert!(!h.state().is_dirty());
    assert!(!asking(&h));
    let kept = versions(&file);
    assert!(kept.iter().any(|v| v == "antes\n"), "the text before: {kept:?}");
    assert!(kept.iter().any(|v| v == "de fora\n"), "the new text: {kept:?}");

    // Pearl's own saves aren't taken for someone else's.
    h.event(egui::Event::Text("meu ".to_string()));
    wait(&mut h, "the autosave", |h| !h.state().is_dirty() && std::fs::read_to_string(&file).unwrap().contains("meu "));
    let mine = h.state().text().to_string();
    linger(&mut h);
    assert_eq!(h.state().text(), mine);
    assert!(!asking(&h));

    // A burst of writes ends in its last text.
    for i in 0..5 {
        write_outside(&file, format!("rajada {i}\n").as_bytes());
        std::thread::sleep(Duration::from_millis(40));
    }
    wait(&mut h, "the last of the burst", |h| h.state().text() == "rajada 4\n");

    // Something other than text leaves the editor as it is, and says so.
    write_outside(&file, &[0xff, 0xfe, b'x']);
    wait(&mut h, "the note on what isn't text", |h| h.state().status().contains("doc.typ"));
    linger(&mut h);
    assert_eq!(h.state().text(), "rajada 4\n");

    // So does a file that's gone.
    std::fs::write(&file, "rajada 4\n").unwrap();
    linger(&mut h);
    std::fs::remove_file(&file).unwrap();
    wait(&mut h, "the note on the file gone", |h| h.state().status() == pearl::tr!("changed-outside-gone", name = "doc.typ"));
    assert_eq!(h.state().text(), "rajada 4\n");
    std::fs::write(&file, "rajada 4\n").unwrap();
    linger(&mut h);

    // With unsaved text, nothing comes in: the user is asked, and their text stays.
    h.event(egui::Event::Text("nao salvo ".to_string()));
    write_outside(&file, "do agente\n".as_bytes());
    wait(&mut h, "the question", asking);
    assert!(h.state().text().contains("nao salvo "));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "do agente\n");
    assert!(versions(&file).iter().any(|v| v == "do agente\n"));

    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
