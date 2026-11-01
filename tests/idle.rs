//! An open window with nothing happening must stop redrawing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable as _;
use pearl::app::App;

static READY: AtomicBool = AtomicBool::new(false);

#[test]
fn an_idle_window_stops_redrawing() {
    let app = App::new(None).unwrap();
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(40).build_ui_state(
        |ui, app: &mut App| {
            if READY.load(Ordering::Relaxed) {
                ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                app.show(ui);
            }
        },
        app,
    );
    READY.store(true, Ordering::Relaxed);
    let started = Instant::now();
    while !harness.state().compiled_once() && started.elapsed() < Duration::from_secs(60) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(harness.state().compiled_once());
    harness.try_run().expect("the window keeps asking to redraw while idle");
}

#[test]
fn compile_errors_are_shown_to_the_user() {
    let mut app = App::new(None).unwrap();
    app.set_text("#funcao_inexistente()");
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(40).build_ui_state(
        |ui, app: &mut App| {
            if READY.load(Ordering::Relaxed) {
                ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                app.show(ui);
            }
        },
        app,
    );
    READY.store(true, Ordering::Relaxed);
    let started = Instant::now();
    while !harness.state().compiled_once() && started.elapsed() < Duration::from_secs(60) {
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(3);
    let message =
        harness.query_all_by(|n| n.value().or(n.label()).is_some_and(|v| v.contains("unknown variable"))).next().expect("the message is on screen");
    let status =
        harness.query_all_by(|n| n.value().or(n.label()).is_some_and(|v| v.contains("Compilado em"))).next().expect("the status is on screen");
    let (message, status) = (message.rect(), status.rect());
    assert!(message.max.y <= status.min.y + 1.0, "the error sits above the status bar: {message:?} {status:?}");
    // Nothing above covers it: the pages' message ends clear of the foot.
    let pages = harness
        .query_all_by(|n| n.label().or(n.value()).is_some_and(|v| v.contains("Corrija o erro")))
        .next()
        .expect("the pages explain why they are empty");
    assert!(pages.rect().max.y <= message.min.y, "{:?} {message:?}", pages.rect());
}

#[test]
fn the_file_view_opens_from_its_tab_and_esc_leaves_it() {
    let app = App::new(None).unwrap();
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(40).build_ui_state(
        |ui, app: &mut App| {
            if READY.load(Ordering::Relaxed) {
                ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                app.show(ui);
            }
        },
        app,
    );
    READY.store(true, Ordering::Relaxed);
    harness.run_steps(3);
    harness.get_by_label("Arquivo").click();
    harness.run_steps(3);
    assert!(harness.query_by_label("Voltar").is_some(), "the File view is open");
    assert!(harness.query_by_label("Salvamento Automático").is_none(), "it covers the window");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(harness.query_by_label("Voltar").is_none(), "Esc closed it");
    assert!(harness.query_by_label("Arquivo").is_some());
}
