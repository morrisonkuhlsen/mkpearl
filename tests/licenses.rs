//! What ships with Pearl says what it is made of, and keeps up with Cargo.lock.

#[allow(dead_code)]
#[path = "../examples/licenses.rs"]
mod generator;

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

#[test]
fn the_licence_file_is_up_to_date() {
    let written = pearl::licenses::TEXT.replace("\r\n", "\n");
    let fresh = generator::generate(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(written == fresh, "THIRD-PARTY-LICENSES.txt is behind: run `cargo run --example licenses`");
}

#[test]
fn it_carries_what_the_licences_ask_for() {
    let text = pearl::licenses::TEXT;
    for needed in [
        "typst 0.15.1: Apache-2.0",
        // typst-assets' notice, with the terms of the fonts inside the program.
        "typst-assets 0.15.1 (NOTICE)",
        "NewCM10-Regular font in files/fonts/NewCM10-Regular.otf",
        "Distribution Exception (DE)",
        "Libertinus Serif fonts in files/fonts/LibertinusSerif-*.otf",
        "Foxit fonts in files/fonts/Foxit*.pfb",
        "hayagriva",
        "CC",
        "option-ext 0.2.0: MPL-2.0",
        "epaint_default_fonts 0.37.0 (fonts/UFL.txt)",
        "Fonts in Pearl's interface: Inter",
        "Fonts in Pearl's interface: JetBrains Mono",
        "Fonts in Pearl's interface: Patrick Hand",
        "Apache License 2.0, standard text",
    ] {
        assert!(text.contains(needed), "{needed}");
    }
}

#[test]
fn the_text_can_be_read_in_the_program() {
    let mut app = App::new(None).unwrap();
    app.open_licenses();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    h.run_steps(3);
    let _ = h.get_by_label("Pearl and what it is made of");
    assert!(pearl::licenses::lines().len() > 10_000);
}
