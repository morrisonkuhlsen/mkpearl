//! The timeline of the writing through the window: moving it changes nothing, a deleted passage comes back
//! as one undo step with the rest as it is, and a damaged version is skipped with a notice.

use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::history::{Kind, Store, now_ms};

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} {:?}", h.state().timeline_position(), h.state().timeline_passages());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

const FIRST: &str = "= Título\nUm.\n\nO parágrafo apagado.\n\nTrês.\n";
const SECOND: &str = "= Título\nUm.\n\nTrês, reescrito.\n";
const NOW: &str = "= Título\nUm.\n\nTrês, reescrito.\n\nQuatro.\n";

#[test]
fn a_past_passage_comes_back_and_the_slider_changes_nothing() {
    let root = std::env::temp_dir().join(format!("pearl-timeline-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    std::fs::write(&file, NOW).unwrap();
    let store = Store::for_document(&file).unwrap();
    let now = now_ms();
    assert!(store.keep(&file, FIRST, Kind::Saved, now - 90_000).unwrap());
    assert!(store.keep(&file, "Vai ser danificada.\n", Kind::Saved, now - 60_000).unwrap());
    assert!(store.keep(&file, SECOND, Kind::Saved, now - 30_000).unwrap());
    // Bytes that aren't text, in the second of them.
    let damaged = std::fs::read_dir(root.join("data/pearl/history")).unwrap().flatten().next().unwrap().path();
    let name = format!("{}", now - 60_000);
    let victim = std::fs::read_dir(&damaged).unwrap().flatten().find(|e| e.file_name().to_string_lossy().contains(&name)).unwrap().path();
    std::fs::write(victim, [0xff, 0xfe, 0x00, 0xc3]).unwrap();

    let mut app = App::new(Some(file)).unwrap();
    app.open_side(6);
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the pages", |a| a.page_count() > 0);
    h.run_steps(3);
    h.get_by_label("Linha do tempo da escrita").click();
    h.run_steps(3);
    let (index, total, _) = h.state().timeline_position().unwrap();
    assert!(total >= 3, "{total}");
    assert_eq!(index, total - 1, "it opens on the newest");
    until(&mut h, "the newest version", |a| a.timeline_passages().is_some());

    // Back to the first version: its pages and what it had, and the document as it was.
    h.state_mut().move_timeline(0);
    until(&mut h, "the first version", |a| a.timeline_pages() == Some(Some(1)) && a.timeline_passages().is_some());
    h.run_steps(3);
    assert_eq!(h.state().text(), NOW, "moving the slider never changes the document");
    assert!(!h.state().is_dirty());
    let passages = h.state().timeline_passages().unwrap();
    assert_eq!(passages, ["O parágrafo apagado.", "Três."]);

    // Passing over the damaged one skips it, says so, and goes on.
    h.state_mut().move_timeline(1);
    until(&mut h, "the damaged version", |a| a.timeline_position().is_some_and(|(_, _, damaged)| damaged == 1));
    let (index, _, _) = h.state().timeline_position().unwrap();
    assert!(index > 1, "it moved past: {index}");
    h.run_steps(3);
    assert!(h.query_by_label("Versões danificadas puladas: 1").is_some());
    assert_eq!(h.state().text(), NOW);

    // One passage comes back, in its place, and the rest stays: one undo takes it away again.
    h.state_mut().move_timeline(0);
    until(&mut h, "the first version again", |a| a.timeline_passages().is_some_and(|p| p.len() == 2));
    h.run_steps(3);
    h.get_all_by_label("Trazer de volta").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().text(), "= Título\nUm.\n\nO parágrafo apagado.\n\nTrês, reescrito.\n\nQuatro.\n");
    until(&mut h, "the shorter list", |a| a.timeline_passages().is_some_and(|p| p == ["Três."]));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut h, "the undo", |a| a.text() == NOW);

    // Seeing a version on the pages takes over from the timeline, and it opens again.
    h.state_mut().view_version(0);
    h.state_mut().preview_version(true);
    h.run_steps(3);
    assert!(h.state().timeline_position().is_none());
    h.state_mut().preview_version(false);
    h.state_mut().open_timeline();
    until(&mut h, "the timeline again", |a| a.timeline_passages().is_some());
    h.run_steps(3);
    assert_eq!(h.state().text(), NOW);

    // Closing goes back to the pages.
    h.run_steps(2);
    h.get_by_label("Fechar").click();
    h.run_steps(3);
    assert!(h.state().timeline_position().is_none());
    assert_eq!(h.state().text(), NOW);

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
