//! Renders a window offscreen to a PNG, without touching the desktop.
//! `cargo run --example shot -- out.png splash` or `… out.png [tab] [dark] [error]`

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui::epaint::text::MissingGlyphPolicy;
use pearl::app::App;
use pearl::splash::{SIZE, SplashScreen};
use pearl::theme::ThemeChoice;

/// Holds the UI back while the harness is built: it runs until idle, which waiting for the first
/// compile would keep it from.
static READY: AtomicBool = AtomicBool::new(false);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().expect("usage: shot out.png [splash | tab] [dark] [error]").clone();
    if args.get(1).is_some_and(|a| a == "splash") {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(SIZE[0], SIZE[1]))
            .with_pixels_per_point(1.5)
            .with_step_dt(0.5)
            .wgpu()
            .build_ui_state(
                |ui, s: &mut SplashScreen| {
                    ui.ctx().set_missing_glyph_policy(MissingGlyphPolicy::Tofu);
                    ui.ctx().set_fonts(pearl::theme::font_definitions());
                    s.ui(ui);
                },
                SplashScreen::default(),
            );
        harness.run_steps(5);
        harness.render().expect("render").save(&out).expect("save");
        println!("{out}");
        return;
    }
    let completing = args.iter().any(|a| a == "complete");
    if args.get(1).is_some_and(|a| a == "session") {
        session_shot(&out);
        return;
    }
    let mut app = App::new(None).expect("start");
    if let Some(tab) = args.get(1) {
        app.set_tab(tab);
    }
    for (flag, side) in [("details", 0), ("outline", 1), ("files", 2), ("symbols", 3), ("problems", 4), ("claude", 5), ("versions", 6)] {
        if args.iter().any(|a| a == flag) {
            app.open_side(side);
        }
    }
    let choice = if args.iter().any(|a| a == "pearl") {
        ThemeChoice::Pearl
    } else if args.iter().any(|a| a == "dark") {
        ThemeChoice::Dark
    } else {
        ThemeChoice::Light
    };
    app.start_with_theme(choice);
    if let Some(file) = args.iter().find_map(|a| a.strip_prefix("open=")) {
        app = App::new(Some(std::path::PathBuf::from(file))).expect("open");
        app.start_with_theme(choice);
        if args.iter().any(|a| a == "versions") {
            app.open_side(6);
        }
    }
    if args.iter().any(|a| a == "licenses") {
        app.open_licenses();
    }
    if args.iter().any(|a| a == "recovery") {
        // Set XDG_DATA_HOME to a folder of its own for this: it looks there for what a run left.
        app.enable_recovery();
    }
    if args.iter().any(|a| a == "file") {
        app.open_file_view();
    }
    if args.iter().any(|a| a == "pages") {
        app.show_pages_only();
    }
    if args.iter().any(|a| a == "error") {
        app.set_text(&format!("{}\n#funcao_inexistente()\n", pearl::sample::SAMPLE));
    }
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1900.0, 1040.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        move |ui, app: &mut App| {
            if !READY.load(Ordering::Relaxed) {
                return;
            }
            // Like the app itself; the harness panics on a missing glyph instead.
            ui.ctx().set_missing_glyph_policy(MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    harness.input_mut().max_texture_side = Some(8192);
    READY.store(true, Ordering::Relaxed);
    let started = Instant::now();
    while !harness.state().compiled_once() && started.elapsed() < Duration::from_secs(60) {
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    harness.run_steps(90);
    if args.iter().any(|a| a == "hover") {
        harness.event(egui::Event::PointerMoved(egui::pos2(8.0 + 44.0 + 46.0 + 4.0 + 3.0 * 7.8, 237.0)));
        for _ in 0..80 {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if args.iter().any(|a| a == "versions") {
        harness.state_mut().view_version(1);
        harness.state_mut().preview_version(true);
        for _ in 0..40 {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if args.iter().any(|a| a == "search") {
        harness.state_mut().open_search();
        harness.step();
        harness.step();
        harness.event(egui::Event::Text("tabela".to_string()));
        for _ in 0..10 {
            harness.step();
        }
    }
    if completing {
        harness.event(egui::Event::Text("#set text(".to_string()));
        for _ in 0..40 {
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(4);
    }
    println!("{}", harness.state().status());
    harness.render().expect("render").save(&out).expect("save");
    println!("{out}");
}

/// The guest's window in a session, with the host's caret on it.
fn session_shot(out: &str) {
    let dir = std::env::temp_dir().join(format!("pearl-shot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("folder");
    let file = dir.join("doc.typ");
    std::fs::write(&file, pearl::sample::SAMPLE).expect("file");
    let mut host = App::new(Some(file)).expect("start");
    host.start_with_theme(ThemeChoice::Dark);
    let link = host.host_session_on("127.0.0.1:0", "Johanna").expect("host");
    let mut guest = App::new(None).expect("start");
    guest.start_with_theme(ThemeChoice::Dark);
    let policy = |ui: &egui::Ui| ui.ctx().set_missing_glyph_policy(MissingGlyphPolicy::Tofu);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(900.0, 600.0)).with_accessibility_check(false).build_ui_state(
        move |ui, a: &mut App| {
            policy(ui);
            if READY.load(Ordering::Relaxed) {
                a.show(ui);
            }
        },
        host,
    );
    let mut g = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1500.0, 800.0))
        .with_pixels_per_point(1.25)
        .wgpu()
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, a: &mut App| {
                policy(ui);
                if READY.load(Ordering::Relaxed) {
                    a.show(ui);
                }
            },
            guest,
        );
    READY.store(true, Ordering::Relaxed);
    h.step();
    let ctx = g.ctx.clone();
    g.state_mut().join_session_link(&ctx, &link, "Egon");
    let started = Instant::now();
    while (g.state().session_carets().is_empty() || g.state().page_count() == 0) && started.elapsed() < Duration::from_secs(30) {
        h.step();
        g.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    h.state_mut().set_text(&format!("{}\nAgora escrevendo juntos.", pearl::sample::SAMPLE));
    for _ in 0..40 {
        h.step();
        g.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    g.input_mut().max_texture_side = Some(8192);
    g.render().expect("render").save(out).expect("save");
    println!("{out}");
    drop(g);
    drop(h);
    let _ = std::fs::remove_dir_all(dir);
}
