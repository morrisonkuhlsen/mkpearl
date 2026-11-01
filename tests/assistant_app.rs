//! The window with Claude in it: from an empty session to edits in the document and back out,
//! against a pretend API.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use pearl::app::App;
use serde_json::{Value, json};

fn sse(events: &[Value]) -> String {
    events.iter().map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap())).collect()
}

fn replace_call() -> String {
    sse(&[
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 3}}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "t", "name": "replace_text", "input": {}}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"old_text\":\"velho\",\"new_text\":\"novo\"}"}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 2}}),
        json!({"type": "message_stop"}),
    ])
}

fn final_words() -> String {
    sse(&[
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 3}}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Troquei."}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 2}}),
        json!({"type": "message_stop"}),
    ])
}

#[test]
fn asking_claude_alone_edits_the_document_and_undoing_takes_it_back() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for reply in [replace_call(), final_words()] {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some(n) = line.to_lowercase().strip_prefix("content-length: ") {
                    length = n.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut out = stream;
            write!(out, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{reply}").unwrap();
        }
    });
    let config = std::env::temp_dir().join(format!("pearl-claude-{}", std::process::id()));
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("ANTHROPIC_API_KEY", "chave-de-teste");
        std::env::set_var("ANTHROPIC_BASE_URL", &url);
    }
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        App::new(None).unwrap(),
    );
    h.state_mut().set_text("texto velho aqui\n");
    h.state_mut().open_side(5);
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().ask_claude(&ctx, "Troque velho por novo");
    assert!(h.state().claude_is_busy());
    let wait = |h: &mut Harness<'_, App>, what: &str, done: &dyn Fn(&App) -> bool| {
        let started = Instant::now();
        while !done(h.state()) {
            assert!(started.elapsed() < Duration::from_secs(30), "{what}: {:?} / {:?}", h.state().text(), h.state().claude_said());
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(&mut h, "the edit", &|a| a.text() == "texto novo aqui\n");
    wait(&mut h, "the end of the request", &|a| !a.claude_is_busy());
    wait(&mut h, "its caret", &|a| a.session_carets().iter().any(|c| c.name == "Claude"));
    let said = h.state().claude_said();
    assert_eq!(said.first().map(String::as_str), Some("Troque velho por novo"));
    assert!(said.iter().any(|s| s == "Editando o documento"), "{said:?}");
    assert!(said.iter().any(|s| s == "Troquei."), "{said:?}");
    assert_eq!(h.state().session_others(), ["Claude"]);
    // The panel, its conversation and its token count draw without trouble.
    h.run_steps(3);
    let _ = std::fs::remove_dir_all(config);
}
