//! Comments through whole windows: tied to a passage, kept beside the document and not in it, and shared
//! with the others in a session, over a real connection.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

fn window<'a>(app: App) -> Harness<'a, App> {
    Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

fn until2(a: &mut Harness<'_, App>, b: &mut Harness<'_, App>, what: &str, done: impl Fn(&App, &App) -> bool) {
    let started = Instant::now();
    while !done(a.state(), b.state()) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} / {:?}", a.state().comment_threads(), b.state().comment_threads());
        a.step();
        b.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn select(h: &mut Harness<'_, App>, word: &str) {
    let ctx = h.ctx.clone();
    let text = h.state().text().to_string();
    let at = text.find(word).unwrap();
    let a = text[..at].chars().count();
    h.state_mut().select(&ctx, a, a + word.chars().count());
    h.run_steps(3);
}

const BODY: &str = "= Título\nUm trecho para comentar, aqui.\n\nOutro parágrafo.\n";

#[test]
fn comments_are_tied_to_a_passage_kept_beside_the_document_and_shared() {
    let root = std::env::temp_dir().join(format!("pearl-comments-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let file = root.join("doc.typ");
    let sidecar = root.join("doc.typ.comments.json");
    std::fs::write(&file, BODY).unwrap();
    let mut app = App::new(Some(file.clone())).unwrap();
    app.open_side(9);
    let mut h = window(app);
    h.run_steps(5);
    assert!(h.query_by_label("Nenhum comentário ainda.").is_some());

    // With nothing selected, no comment; with a passage, one, and the document isn't touched.
    let ctx = h.ctx.clone();
    assert!(!h.state_mut().comment_on_selection(&ctx, "Sem seleção."));
    select(&mut h, "trecho para comentar");
    assert!(h.state_mut().comment_on_selection(&ctx, "Reveja esta frase."));
    h.run_steps(3);
    assert_eq!(h.state().text(), BODY);
    assert!(!h.state().is_dirty(), "a comment is no change to the document");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), BODY);
    assert!(sidecar.is_file(), "kept at once");
    assert!(h.query_by_label("Reveja esta frase.").is_some());
    let threads = h.state().comment_threads();
    assert_eq!(threads[0].0, "trecho para comentar");
    assert!(threads[0].1, "in the text");

    // Replying through the pane; the passage moves with what's written before it.
    h.state_mut().reply_to_comment(0, "Concordo.");
    h.state_mut().set_text(&format!("Novo começo.\n\n{BODY}"));
    h.run_steps(3);
    assert!(h.state().comment_threads()[0].1);

    // A later run brings them back, wherever the passage has gone.
    h.state_mut().set_text(BODY);
    h.run_steps(3);
    drop(h);
    let moved = format!("Outro começo.\n\n{BODY}");
    std::fs::write(&file, &moved).unwrap();
    let mut app = App::new(Some(file.clone())).unwrap();
    app.open_side(9);
    let mut h = window(app);
    h.run_steps(5);
    let threads = h.state().comment_threads();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].3.len(), 2);
    assert!(threads[0].1, "found again in the text");

    // Going to it selects the passage.
    let ctx = h.ctx.clone();
    h.state_mut().go_to_comment(&ctx, 0);
    h.run_steps(3);
    let (a, b) = h.state().selected_range(&ctx);
    assert_eq!(moved.chars().skip(a).take(b - a).collect::<String>(), "trecho para comentar");

    // A guest gets them on arriving, and what each one says reaches the other.
    let link = h.state_mut().host_session_on("127.0.0.1:0", "Ana").unwrap();
    let mut guest = App::new(None).unwrap();
    guest.open_side(9);
    let mut g = window(guest);
    let ctx_g = g.ctx.clone();
    g.state_mut().join_session_link(&ctx_g, &link, "Bia");
    until2(&mut h, &mut g, "the guest's copy", |a, b| a.text() == b.text() && b.comment_threads().len() == 1);
    assert_eq!(g.state().comment_threads()[0].3[0].1, "Reveja esta frase.");

    // Replies at the same moment both stay.
    h.state_mut().reply_to_comment(0, "Da anfitriã.");
    g.state_mut().reply_to_comment(0, "Da convidada.");
    until2(&mut h, &mut g, "both replies", |a, b| a.comment_threads()[0].3.len() == 4 && b.comment_threads()[0].3.len() == 4);
    assert_eq!(h.state().comment_threads(), g.state().comment_threads());

    // A guest's new thread reaches the host, whose file keeps it.
    select(&mut g, "Outro parágrafo");
    assert!(g.state_mut().comment_on_selection(&ctx_g, "E este?"));
    until2(&mut h, &mut g, "the guest's thread", |a, _| a.comment_threads().len() == 2);
    h.run_steps(3);
    let kept = std::fs::read_to_string(&sidecar).unwrap();
    assert!(kept.contains("E este?") && kept.contains("Da convidada."), "{kept}");
    assert!(!g.state().is_dirty() || g.state().text() == moved);

    // Resolved by one, for both; deleted, and the file follows.
    g.state_mut().resolve_comment(1, true);
    until2(&mut h, &mut g, "resolved", |a, _| a.comment_threads().get(1).is_some_and(|t| t.2));
    h.state_mut().delete_comment(1);
    until2(&mut h, &mut g, "deleted", |a, b| a.comment_threads().len() == 1 && b.comment_threads().len() == 1);

    // Leaving, with all deleted, the file goes too. The document stays.
    drop(g);
    h.state_mut().leave_session();
    h.state_mut().delete_comment(0);
    h.run_steps(3);
    assert!(!sidecar.exists());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), moved);

    // A damaged file is kept aside, not written over, and says so.
    drop(h);
    std::fs::write(&sidecar, "{ isto não é json").unwrap();
    let mut h = window(App::new(Some(file)).unwrap());
    h.run_steps(5);
    assert!(h.state().comment_threads().is_empty());
    assert!(!sidecar.exists());
    assert_eq!(std::fs::read_to_string(root.join("doc.typ.comments.json.damaged")).unwrap(), "{ isto não é json");
    assert!(h.state().status().contains("danificado"), "{}", h.state().status());

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
