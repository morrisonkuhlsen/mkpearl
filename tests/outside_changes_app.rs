//! A file changed by something other than Pearl: what it says is never written over unseen.

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

fn asking(h: &Harness<'_, App>) -> bool {
    h.query_by_label(tx("use-the-files-text")).is_some()
}

fn type_and_wait_for_the_question(h: &mut Harness<'_, App>, typed: &str) {
    h.event(egui::Event::Text(typed.to_string()));
    wait(h, "the question", asking);
}

fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

fn versions(file: &Path) -> Vec<String> {
    let store = Store::for_document(file).unwrap();
    store.list().iter().filter_map(|v| store.read(v)).collect()
}

fn read(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap()
}

#[test]
fn what_changed_outside_is_kept_and_asked_about() {
    let root = std::env::temp_dir().join(format!("pearl-outside-{}", std::process::id()));
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

    // Changed before the first edit of a file opened at the start, the issue's own steps.
    std::fs::write(&file, "de fora 0\n").unwrap();
    type_and_wait_for_the_question(&mut h, "zero ");
    assert_eq!(read(&file), "de fora 0\n");
    assert!(versions(&file).iter().any(|v| v == "de fora 0\n"));
    click(&mut h, tx("write-mine-over-the-file"));
    assert!(read(&file).contains("zero "));

    // The same bytes written again, as a sync folder does, are no change at all.
    std::fs::write(&file, read(&file)).unwrap();
    h.event(egui::Event::Text("um ".to_string()));
    wait(&mut h, "the autosave", |h| !h.state().is_dirty() && read(&file).contains("um "));

    // Autosave doesn't write over the outside text, and it's already in the history when asked.
    std::fs::write(&file, "de fora 1\n").unwrap();
    type_and_wait_for_the_question(&mut h, "dois ");
    assert_eq!(read(&file), "de fora 1\n");
    assert!(versions(&file).iter().any(|v| v == "de fora 1\n"));
    assert!(h.state().is_dirty());

    // While the question stands, nothing is written, however long it takes.
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(read(&file), "de fora 1\n");

    // Writing mine over it does just that, and autosave goes on working.
    let mine = h.state().text().to_string();
    click(&mut h, tx("write-mine-over-the-file"));
    assert_eq!(read(&file), mine);
    assert!(!asking(&h));
    h.event(egui::Event::Text("tres ".to_string()));
    wait(&mut h, "autosave after the answer", |h| !h.state().is_dirty() && read(&file).contains("tres "));

    // A newer change while the question stands asks again instead of being written over.
    std::fs::write(&file, "de fora 2\n").unwrap();
    type_and_wait_for_the_question(&mut h, "quatro ");
    std::fs::write(&file, "de fora 3\n").unwrap();
    click(&mut h, tx("write-mine-over-the-file"));
    h.run_steps(2);
    assert!(asking(&h), "the newer change is asked about");
    assert_eq!(read(&file), "de fora 3\n");
    assert!(versions(&file).iter().any(|v| v == "de fora 3\n"));

    // Using the file's text brings it in with nothing unsaved, and mine stays in the history.
    let mine = h.state().text().to_string();
    click(&mut h, tx("use-the-files-text"));
    assert!(!asking(&h));
    assert_eq!(h.state().text(), "de fora 3\n");
    assert!(!h.state().is_dirty());
    assert!(versions(&file).contains(&mine), "my text is in the history");

    // A file that's gone holds nothing to lose: it's written again without asking.
    std::fs::remove_file(&file).unwrap();
    h.event(egui::Event::Text("volta ".to_string()));
    wait(&mut h, "the file again", |h| !h.state().is_dirty() && file.exists());
    assert!(!asking(&h));

    // One too large to keep in the history is still asked about, and only written over on request.
    let count = versions(&file).len();
    std::fs::File::create(&file).unwrap().set_len(101 * 1024 * 1024).unwrap();
    type_and_wait_for_the_question(&mut h, "grande ");
    assert_eq!(std::fs::metadata(&file).unwrap().len(), 101 * 1024 * 1024);
    assert!(versions(&file).len() <= count + 1, "no huge version");
    click(&mut h, tx("write-mine-over-the-file"));
    assert!(read(&file).contains("grande"));

    // Not now leaves both as they are and stops autosave asking every second.
    std::fs::write(&file, "de fora 4\n").unwrap();
    type_and_wait_for_the_question(&mut h, "cinco ");
    click(&mut h, tx("not-now"));
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!asking(&h));
    assert!(h.state().is_dirty());
    assert!(h.state().text().contains("cinco"));
    assert_eq!(read(&file), "de fora 4\n");
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}
