//! Claude in a real session, against a pretend API: no credits are spent, and what the app sends is
//! checked here.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pearl::assistant::api::{Connection, Credential};
use pearl::assistant::{Assistant, Update};
use pearl::collab::Session;
use serde_json::{Value, json};

/// One scripted reply: the status and the events.
struct Reply {
    status: u16,
    events: Vec<Value>,
}

struct Seen {
    headers: Vec<String>,
    body: Value,
}

/// Answers each request with the next reply, keeping what it was sent.
fn pretend_api(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for reply in replies {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut headers = Vec::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let line = line.trim_end().to_string();
                if line.is_empty() {
                    break;
                }
                if let Some(n) = line.to_lowercase().strip_prefix("content-length: ") {
                    length = n.parse().unwrap();
                }
                headers.push(line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            log.lock().unwrap().push(Seen { headers, body: serde_json::from_slice(&body).unwrap() });
            let mut out = stream;
            if reply.status == 200 {
                write!(out, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n").unwrap();
                for e in &reply.events {
                    write!(out, "event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()).unwrap();
                    out.flush().unwrap();
                }
            } else {
                let error = json!({"type": "error", "error": {"type": "x", "message": "recusado"}}).to_string();
                write!(out, "HTTP/1.1 {} X\r\ncontent-length: {}\r\nretry-after: 0\r\nconnection: close\r\n\r\n{error}", reply.status, error.len())
                    .unwrap();
            }
        }
    });
    (url, seen)
}

fn start(index: u64, block: Value) -> Value {
    json!({"type": "content_block_start", "index": index, "content_block": block})
}

fn stop(index: u64) -> Value {
    json!({"type": "content_block_stop", "index": index})
}

fn finish(reason: &str) -> Vec<Value> {
    vec![json!({"type": "message_delta", "delta": {"stop_reason": reason}, "usage": {"output_tokens": 5}}), json!({"type": "message_stop"})]
}

/// A reply that calls one tool, after a thinking block to be echoed back.
fn calls(id: &str, tool: &str, input: &Value) -> Reply {
    let mut events = vec![
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 9, "cache_read_input_tokens": 3}}}),
        start(0, json!({"type": "thinking", "thinking": "", "signature": ""})),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "assinatura"}}),
        stop(0),
        start(1, json!({"type": "tool_use", "id": id, "name": tool, "input": {}})),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": input.to_string()}}),
        stop(1),
    ];
    events.extend(finish("tool_use"));
    Reply { status: 200, events }
}

fn says(text: &str) -> Reply {
    let mut events = vec![
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 1}}}),
        start(0, json!({"type": "text", "text": ""})),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}}),
        stop(0),
    ];
    events.extend(finish("end_turn"));
    Reply { status: 200, events }
}

fn stops_with(reason: &str, details: Value) -> Reply {
    Reply {
        status: 200,
        events: vec![
            json!({"type": "message_start", "message": {"usage": {}}}),
            json!({"type": "message_delta", "delta": {"stop_reason": reason, "stop_details": details}, "usage": {}}),
            json!({"type": "message_stop"}),
        ],
    }
}

fn claude(host: &Session, url: String) -> Assistant {
    Assistant::start(host.link().to_string(), Connection { url, credential: Credential::ApiKey("chave-de-teste".to_string()) }).unwrap()
}

/// Polls the session until Claude says `done`, returning all it said.
fn until(host: &mut Session, claude: &Assistant, done: impl Fn(&Update) -> bool) -> Vec<Update> {
    let started = Instant::now();
    let mut said = Vec::new();
    loop {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out; said {said:?}, text {:?}", host.text());
        host.poll();
        for update in claude.updates.try_iter() {
            let finished = done(&update);
            said.push(update);
            if finished {
                return said;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn host_with(text: &str) -> Session {
    Session::host_on("127.0.0.1:0", text, "Ana", "doc.typ").unwrap()
}

fn is_end(u: &Update) -> bool {
    matches!(u, Update::Done | Update::Problem(_) | Update::Stopped | Update::Gone(_))
}

#[test]
fn claude_edits_the_document_as_a_person_with_a_name_and_caret() {
    let (url, seen) = pretend_api(vec![
        calls("t1", "read_document", &json!({})),
        calls("t2", "replace_text", &json!({"old_text": "mundo", "new_text": "pessoal querido"})),
        says("Troquei a saudação."),
    ]);
    let mut host = host_with("olá mundo, até logo");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Troque a saudação".to_string(), None);
    let said = until(&mut host, &c, is_end);
    assert!(said.contains(&Update::Done), "{said:?}");
    assert!(said.contains(&Update::Text("Troquei a saudação.".to_string())));
    let started = Instant::now();
    while host.text() != "olá pessoal querido, até logo" {
        assert!(started.elapsed() < Duration::from_secs(10), "{:?}", host.text());
        host.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.others(), ["Claude"]);
    assert_eq!(host.cursors()[0].name, "Claude");

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    let head = &seen[0].headers;
    let has = |h: &[String], line: &str| h.iter().any(|x| x.eq_ignore_ascii_case(line));
    assert!(has(head, "x-api-key: chave-de-teste"), "{head:?}");
    assert!(has(head, "anthropic-version: 2026-10-21"), "{head:?}");
    assert!(has(head, "anthropic-beta: server-side-fallback-2026-07-01"), "{head:?}");
    assert_eq!(seen[0].body["model"], "claude-opus-5-5");

    // The tool's result goes back, with the thinking block echoed untouched before it.
    let messages = seen[1].body["messages"].as_array().unwrap();
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0]["type"], "thinking");
    assert_eq!(messages[1]["content"][0]["signature"], "assinatura");
    assert_eq!(messages[2]["content"][0]["tool_use_id"], "t1");
    assert_eq!(messages[2]["content"][0]["content"], "olá mundo, até logo");

    // The history only grows, so the cached prefix and thinking blocks stay valid.
    let later = seen[2].body["messages"].as_array().unwrap();
    assert_eq!(&later[..messages.len()], &messages[..]);
    assert_eq!(seen[0].body["system"], seen[2].body["system"]);
    assert_eq!(seen[0].body["tools"], seen[2].body["tools"]);
}

#[test]
fn what_someone_types_meanwhile_is_kept_and_undo_takes_back_only_claudes_part() {
    let long = "novo trecho ".repeat(30);
    let (url, _) = pretend_api(vec![calls("t", "append_text", &json!({"text": long})), says("Pronto.")]);
    let mut host = host_with("início\n");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Escreva".to_string(), None);
    // Types at the top while Claude is writing at the end.
    let started = Instant::now();
    while host.text() == "início\n" {
        assert!(started.elapsed() < Duration::from_secs(10));
        host.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let now = format!("ANA {}", host.text());
    host.sync_local(&now);
    until(&mut host, &c, is_end);
    let expected = format!("ANA início\n{long}");
    let started = Instant::now();
    while host.text() != expected {
        assert!(started.elapsed() < Duration::from_secs(10), "{:?}", host.text());
        host.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    c.undo_all();
    until(&mut host, &c, |u| *u == Update::Undone);
    let started = Instant::now();
    while host.text() != "ANA início\n" {
        assert!(started.elapsed() < Duration::from_secs(10), "{:?}", host.text());
        host.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_failed_edit_and_a_broken_input_go_back_as_errors_for_claude_to_fix() {
    let broken = Reply {
        status: 200,
        events: {
            let mut e = vec![
                json!({"type": "message_start", "message": {"usage": {}}}),
                start(0, json!({"type": "tool_use", "id": "ruim", "name": "replace_text", "input": {}})),
                json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"old_text\": \"a"}}),
                stop(0),
            ];
            e.extend(finish("tool_use"));
            e
        },
    };
    let (url, seen) = pretend_api(vec![
        calls("t1", "replace_text", &json!({"old_text": "não existe", "new_text": "x"})),
        broken,
        calls("t3", "nada_disso", &json!({})),
        says("ok"),
    ]);
    let mut host = host_with("texto");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Tente".to_string(), None);
    let said = until(&mut host, &c, is_end);
    assert!(said.contains(&Update::Done), "{said:?}");
    assert_eq!(host.text(), "texto", "nothing was changed");
    let seen = seen.lock().unwrap();
    let result = |request: usize| {
        let messages = seen[request].body["messages"].as_array().unwrap();
        messages.last().unwrap()["content"][0].clone()
    };
    assert_eq!(result(1)["is_error"], true);
    assert!(result(1)["content"].as_str().unwrap().contains("isn't in the document"));
    assert_eq!(result(2)["is_error"], true);
    assert!(result(2)["content"].as_str().unwrap().contains("INVALID_JSON"));
    assert_eq!(result(3)["is_error"], true);
}

#[test]
fn a_tool_call_cut_off_at_the_limit_does_not_run() {
    let mut events = vec![
        json!({"type": "message_start", "message": {"usage": {}}}),
        start(0, json!({"type": "tool_use", "id": "t", "name": "append_text", "input": {}})),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"text\": \"meio\"}"}}),
        stop(0),
    ];
    events.extend(finish("max_tokens"));
    let (url, seen) = pretend_api(vec![Reply { status: 200, events }, says("ok")]);
    let mut host = host_with("x");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Escreva".to_string(), None);
    let said = until(&mut host, &c, is_end);
    assert!(matches!(said.last(), Some(Update::Problem(_))), "{said:?}");
    assert_eq!(host.text(), "x");
    // And the conversation can go on: nothing half-finished stayed in the history.
    c.ask("De novo".to_string(), None);
    assert!(until(&mut host, &c, is_end).contains(&Update::Done));
    let seen = seen.lock().unwrap();
    let roles: Vec<&str> = seen[1].body["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "user"]);
}

#[test]
fn a_refusal_is_reported_with_its_category() {
    let (url, _) = pretend_api(vec![stops_with("refusal", json!({"type": "refusal", "category": "cyber"}))]);
    let mut host = host_with("x");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Algo".to_string(), None);
    let said = until(&mut host, &c, is_end);
    match said.last() {
        Some(Update::Problem(message)) => assert!(message.contains("cyber"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_bad_key_says_so_without_retrying_and_overload_retries() {
    let (url, seen) = pretend_api(vec![Reply { status: 401, events: vec![] }]);
    let mut host = host_with("x");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Algo".to_string(), None);
    let said = until(&mut host, &c, is_end);
    match said.last() {
        Some(Update::Problem(message)) => assert!(message.contains("chave"), "{message}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(seen.lock().unwrap().len(), 1);

    let (url, seen) = pretend_api(vec![Reply { status: 529, events: vec![] }, says("voltei")]);
    let mut host = host_with("x");
    let c = self::claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Algo".to_string(), None);
    assert!(until(&mut host, &c, is_end).contains(&Update::Done));
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn the_selection_goes_with_the_request() {
    let (url, seen) = pretend_api(vec![says("ok")]);
    let mut host = host_with("x");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Melhore isto".to_string(), Some("trecho escolhido".to_string()));
    until(&mut host, &c, is_end);
    let seen = seen.lock().unwrap();
    let text = seen[0].body["messages"][0]["content"].as_str().unwrap();
    assert!(text.starts_with("Melhore isto") && text.contains("trecho escolhido"), "{text}");
}

#[test]
fn checking_the_document_compiles_it_and_reports_errors_with_their_line() {
    let (url, seen) = pretend_api(vec![calls("t", "check_document", &json!({})), says("ok")]);
    let mut host = host_with("ok\n#nao_existe\n");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Verifique".to_string(), None);
    assert!(until(&mut host, &c, is_end).contains(&Update::Done));
    let seen = seen.lock().unwrap();
    let report = seen[1].body["messages"].as_array().unwrap().last().unwrap()["content"][0]["content"].as_str().unwrap().to_string();
    assert!(report.contains("error on line 2"), "{report}");
}

#[test]
fn stopping_ends_the_request_and_leaving_ends_the_caret() {
    let (url, _) = pretend_api(vec![calls("t", "read_document", &json!({}))]);
    let mut host = host_with("x");
    let c = claude(&host, url);
    until(&mut host, &c, |u| *u == Update::Connected);
    c.ask("Algo".to_string(), None);
    c.stop();
    let said = until(&mut host, &c, is_end);
    assert!(matches!(said.last(), Some(Update::Stopped | Update::Problem(_))), "{said:?}");
    c.ask("n".to_string(), None);
    c.stop();
    drop(c);
    let started = Instant::now();
    while !host.others().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(20));
        host.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}
