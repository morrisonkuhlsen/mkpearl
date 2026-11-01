//! Spell checking through the window: a wrong word is found, its menu offers the fix, and the fix
//! lands in the text.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn wait(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(started.elapsed() < Duration::from_secs(60), "{what}: {:?}", h.state().misspelt_words());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_wrong_word_is_found_and_fixed_and_the_rest_is_left_alone() {
    let root = std::env::temp_dir().join(format!("pearl-spell-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets it before any thread reads it.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(None).unwrap(),
    );
    h.state_mut().set_text("O cachoro correu. #text(\"cachoro\") xyzzy\n");
    wait(&mut h, "the wrong words", |a| a.misspelt_words() == ["cachoro", "xyzzy"]);

    h.state_mut().open_spell_menu(4, egui::pos2(300.0, 300.0));
    h.step();
    wait(&mut h, "the suggestions", |_| true);
    let started = Instant::now();
    while h.query_by_label("cachorro").is_none() {
        assert!(started.elapsed() < Duration::from_secs(60), "no suggestion");
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    h.get_by_label("cachorro").click();
    h.run_steps(2);
    assert_eq!(h.state().text(), "O cachorro correu. #text(\"cachoro\") xyzzy\n", "the string in the code isn't touched");
    assert!(h.state().is_dirty());

    // Added to the dictionary: right from now on, and kept.
    wait(&mut h, "after the fix", |a| a.misspelt_words() == ["xyzzy"]);
    let byte = h.state().text().find("xyzzy").unwrap();
    h.state_mut().open_spell_menu(byte, egui::pos2(300.0, 300.0));
    h.run_steps(2);
    h.get_by_label("Adicionar ao dicionário").click();
    h.run_steps(2);
    assert!(h.state().misspelt_words().is_empty());
    assert_eq!(std::fs::read_to_string(root.join("pearl/dictionary.txt")).unwrap(), "xyzzy");

    // Another language: nothing is marked. Off: nothing either.
    h.state_mut().set_text("#set text(lang: \"en\")\nerrro\n");
    h.run_steps(2);
    assert!(h.state().misspelt_words().is_empty());
    h.state_mut().set_text("errro\n");
    wait(&mut h, "the word", |a| a.misspelt_words() == ["errro"]);
    h.state_mut().set_spell(false);
    h.run_steps(2);
    assert!(h.state().misspelt_words().is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
