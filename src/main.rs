//! `pearl [--no-splash] [documento.typ]` opens a document, or the sample without one, after the
//! splash screen.

use pearl::splash::{self, SplashScreen};

struct SplashWindow {
    screen: SplashScreen,
    setup: bool,
}

impl eframe::App for SplashWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.setup {
            self.setup = true;
            ui.ctx().set_fonts(pearl::theme::font_definitions());
        }
        self.screen.ui(ui);
    }
}

fn show_splash() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Pearl")
        .with_app_id("pearl")
        .with_inner_size(splash::SIZE)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_icon(splash::icon());
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Pearl splash", options, Box::new(|_| Ok(Box::new(SplashWindow { screen: SplashScreen::default(), setup: false }))))
}

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let path = args.iter().find(|a| !a.to_string_lossy().starts_with("--")).map(std::path::PathBuf::from);
    // Before the splash, so the fonts and the first compile are ready when the window opens.
    let app = pearl::app::App::new(path).map_err(|e| eframe::Error::AppCreation(e.into()))?;
    if !args.iter().any(|a| a == "--no-splash") {
        show_splash()?;
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Pearl")
        .with_app_id("pearl")
        .with_inner_size([1400.0, 900.0])
        .with_min_inner_size([720.0, 480.0])
        .with_icon(splash::icon());
    if pearl::app::WINDOW_CONTROLS {
        viewport = viewport.with_decorations(false);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Pearl", options, Box::new(move |_| Ok(Box::new(app))))
}
