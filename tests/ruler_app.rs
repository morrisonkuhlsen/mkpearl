//! The rulers through the window: they sit outside the pages, and the choice to hide them is kept.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;

#[test]
fn the_rulers_frame_the_pages_and_can_be_hidden() {
    let root = std::env::temp_dir().join(format!("pearl-ruler-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(None).unwrap(),
    );
    assert!(h.state().ruler_strips().is_none());
    let started = Instant::now();
    while h.state().page_count() == 0 {
        assert!(started.elapsed() < Duration::from_secs(60), "no pages");
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(3);
    let (top, side) = h.state().ruler_strips().expect("on by default");
    assert_eq!(top.height(), pearl::ruler::RULER);
    assert_eq!(side.width(), pearl::ruler::RULER);
    assert!(top.left() >= side.right() && side.top() >= top.bottom(), "the strips don't overlap: {top:?} {side:?}");

    h.state_mut().set_ruler(false);
    h.run_steps(3);
    assert!(h.state().ruler_strips().is_none());
    assert!(App::new(None).unwrap().ruler_strips().is_none());
    assert_eq!(std::fs::read_to_string(root.join("config/pearl/ruler.txt")).unwrap(), "off");

    h.state_mut().set_ruler(true);
    h.run_steps(3);
    assert!(h.state().ruler_strips().is_some());
    let _ = std::fs::remove_dir_all(&root);
}
