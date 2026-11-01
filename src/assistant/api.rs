//! The Messages API over plain HTTP, as there is no official Rust SDK: the request, and reading its
//! Server-Sent Events into the turn's content blocks.

use std::io::BufRead;
use std::time::Duration;

use serde_json::{Value, json};

/// Claude Opus 5.5, the current default.
pub const MODEL: &str = "claude-opus-5-5";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
pub const DEFAULT_URL: &str = "https://api.anthropic.com";
/// Streaming, so a long answer doesn't meet an HTTP timeout.
const MAX_TOKENS: u32 = 64_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest silence on an open stream; the API sends pings well within it.
const READ_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_ATTEMPTS: u32 = 4;

#[derive(Clone)]
pub enum Credential {
    ApiKey(String),
    /// A short-lived OAuth token (`ANTHROPIC_AUTH_TOKEN`).
    Token(String),
}

#[derive(Clone)]
pub struct Connection {
    pub url: String,
    pub credential: Credential,
}

/// The tools, in a fixed order so the prompt's cached prefix stays the same from turn to turn.
pub fn tools() -> Value {
    json!([
        {
            "name": "read_document",
            "description": "Returns the Typst source of the document as it is right now. Others are typing too, so read it again before editing if time has passed.",
            "eager_input_streaming": true,
            "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "replace_text",
            "description": "Replaces one exact piece of the document's source with new text. old_text must match exactly, including line breaks, and occur exactly once: include enough around it to make it unique. If the document is empty, leave old_text empty to write into it.",
            "eager_input_streaming": true,
            "input_schema": {
                "type": "object",
                "properties": {
                    "old_text": {"type": "string", "description": "The exact text to replace."},
                    "new_text": {"type": "string", "description": "What goes in its place."}
                },
                "required": ["old_text", "new_text"],
                "additionalProperties": false
            }
        },
        {
            "name": "append_text",
            "description": "Adds text at the very end of the document.",
            "eager_input_streaming": true,
            "input_schema": {
                "type": "object",
                "properties": {"text": {"type": "string", "description": "The text to add."}},
                "required": ["text"],
                "additionalProperties": false
            }
        },
        {
            "name": "check_document",
            "description": "Compiles the document as it is right now and returns any errors and warnings, each with its line, and the number of pages.",
            "eager_input_streaming": true,
            "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}
        }
    ])
}

pub fn request_body(system: &str, messages: &[Value]) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "thinking": {"type": "adaptive"},
        "output_config": {"effort": "medium"},
        // A classifier's decline is retried on another model by the API itself.
        "fallbacks": "default",
        // Caches up to the last block, so each turn only pays in full for what's new.
        "cache_control": {"type": "ephemeral"},
        "system": system,
        "tools": tools(),
        "messages": messages,
    })
}

/// What one response came to.
#[derive(Debug, Default)]
pub struct Turn {
    pub blocks: Vec<Value>,
    pub stop_reason: Option<String>,
    /// For a refusal: the category the API gives.
    pub refusal_category: Option<String>,
    pub usage: Usage,
    /// Tool inputs that weren't valid JSON, by tool-use id, as they arrived.
    pub invalid_inputs: Vec<(String, String)>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

impl Turn {
    /// The blocks to send back, as the API wants after a fallback: what came before the last switch
    /// is kept only as text, and model-internal blocks from it are left out.
    pub fn echo_blocks(&self) -> Vec<Value> {
        let boundary = self.blocks.iter().rposition(|b| b["type"] == "fallback");
        self.blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| match boundary {
                Some(last) if *i <= last => b["type"] == "text",
                _ => true,
            })
            .map(|(_, b)| b.clone())
            .collect()
    }

    /// The tool calls to run: only those after the last fallback, the rest having been abandoned.
    pub fn tool_uses(&self) -> Vec<Value> {
        let boundary = self.blocks.iter().rposition(|b| b["type"] == "fallback");
        self.blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| b["type"] == "tool_use" && boundary.is_none_or(|last| *i > last))
            .map(|(_, b)| b.clone())
            .collect()
    }
}

/// Puts a response together, event by event.
#[derive(Default)]
pub struct Assembler {
    turn: Turn,
    /// Each open block's tool input as it arrives, by block position.
    partial: Vec<(usize, String)>,
    /// Blocks by the index the stream gives them.
    positions: Vec<(u64, usize)>,
}

fn usage_of(value: &Value) -> Usage {
    let n = |key: &str| value[key].as_u64().unwrap_or(0);
    Usage {
        input: n("input_tokens"),
        output: n("output_tokens"),
        cache_read: n("cache_read_input_tokens"),
        cache_write: n("cache_creation_input_tokens"),
    }
}

impl Assembler {
    fn position(&self, index: u64) -> Option<usize> {
        self.positions.iter().find(|(i, _)| *i == index).map(|(_, p)| *p)
    }

    /// Takes one event's data. Returns text to show as it arrives, or the error the stream reported.
    pub fn event(&mut self, data: &Value) -> Result<Option<String>, String> {
        match data["type"].as_str().unwrap_or_default() {
            "message_start" => self.turn.usage = usage_of(&data["message"]["usage"]),
            "content_block_start" => {
                let index = data["index"].as_u64().unwrap_or(self.positions.len() as u64);
                self.positions.push((index, self.turn.blocks.len()));
                self.turn.blocks.push(data["content_block"].clone());
            }
            "content_block_delta" => {
                let Some(position) = self.position(data["index"].as_u64().unwrap_or(u64::MAX)) else { return Ok(None) };
                let delta = &data["delta"];
                let append = |block: &mut Value, key: &str, field: &str| {
                    let text = format!("{}{}", block[key].as_str().unwrap_or_default(), delta[field].as_str().unwrap_or_default());
                    block[key] = Value::String(text);
                };
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        if let Some(block) = self.turn.blocks.get_mut(position) {
                            append(block, "text", "text");
                        }
                        return Ok(delta["text"].as_str().map(str::to_string));
                    }
                    "thinking_delta" => {
                        if let Some(block) = self.turn.blocks.get_mut(position) {
                            append(block, "thinking", "thinking");
                        }
                    }
                    "signature_delta" => {
                        if let Some(block) = self.turn.blocks.get_mut(position) {
                            append(block, "signature", "signature");
                        }
                    }
                    "input_json_delta" => {
                        let piece = delta["partial_json"].as_str().unwrap_or_default();
                        match self.partial.iter_mut().find(|(p, _)| *p == position) {
                            Some((_, text)) => text.push_str(piece),
                            None => self.partial.push((position, piece.to_string())),
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let Some(position) = self.position(data["index"].as_u64().unwrap_or(u64::MAX)) else { return Ok(None) };
                let raw = self.partial.iter().find(|(p, _)| *p == position).map(|(_, t)| t.clone()).unwrap_or_default();
                if let Some(block) = self.turn.blocks.get_mut(position)
                    && block["type"] == "tool_use"
                {
                    // Eager streaming means the API hasn't checked this input; a tool with no
                    // parameters arrives with none at all.
                    let text = if raw.trim().is_empty() { "{}" } else { raw.as_str() };
                    match serde_json::from_str::<Value>(text) {
                        Ok(input) if input.is_object() => block["input"] = input,
                        _ => {
                            let id = block["id"].as_str().unwrap_or_default().to_string();
                            block["input"] = json!({});
                            self.turn.invalid_inputs.push((id, raw));
                        }
                    }
                }
            }
            "message_delta" => {
                if let Some(reason) = data["delta"]["stop_reason"].as_str() {
                    self.turn.stop_reason = Some(reason.to_string());
                }
                if let Some(category) = data["delta"]["stop_details"]["category"].as_str() {
                    self.turn.refusal_category = Some(category.to_string());
                }
                // The message's final counts: output grows through the stream, input is in the start.
                let last = usage_of(&data["usage"]);
                self.turn.usage.output = last.output.max(self.turn.usage.output);
                self.turn.usage.input = last.input.max(self.turn.usage.input);
                self.turn.usage.cache_read = last.cache_read.max(self.turn.usage.cache_read);
                self.turn.usage.cache_write = last.cache_write.max(self.turn.usage.cache_write);
            }
            "error" => {
                let message = data["error"]["message"].as_str().unwrap_or("erro desconhecido");
                return Err(format!("{}: {message}", data["error"]["type"].as_str().unwrap_or("error")));
            }
            _ => {}
        }
        Ok(None)
    }

    pub fn finish(self) -> Turn {
        self.turn
    }
}

/// Why a request didn't produce a turn.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Stopped,
    Failed(String),
}

fn friendly(status: u16, detail: &str) -> String {
    match status {
        401 => "A chave da API foi recusada. Confira-a no painel do Claude.".to_string(),
        402 => "A conta da API está sem créditos ou com problema de cobrança.".to_string(),
        403 => "Esta chave não tem permissão para usar este modelo.".to_string(),
        404 => "O modelo não está disponível para esta conta.".to_string(),
        413 => "O documento é grande demais para esta requisição.".to_string(),
        _ => format!("A API respondeu {status}: {detail}"),
    }
}

/// Sends the request and reads the stream, with `on_text` seeing the text as it comes and `stopped`
/// asked between events. Overloads and rate limits are retried while nothing has been received.
pub fn stream(connection: &Connection, body: &Value, stopped: &dyn Fn() -> bool, on_text: &mut dyn FnMut(&str)) -> Result<Turn, Failure> {
    let agent = ureq::AgentBuilder::new().timeout_connect(CONNECT_TIMEOUT).timeout_read(READ_TIMEOUT).build();
    let url = format!("{}/v1/messages", connection.url.trim_end_matches('/'));
    let payload = body.to_string();
    let mut attempt = 0;
    let response = loop {
        attempt += 1;
        let request = agent.post(&url).set("content-type", "application/json").set("anthropic-version", API_VERSION);
        let request = match &connection.credential {
            Credential::ApiKey(key) => request.set("x-api-key", key).set("anthropic-beta", FALLBACK_BETA),
            Credential::Token(token) => {
                request.set("authorization", &format!("Bearer {token}")).set("anthropic-beta", &format!("{FALLBACK_BETA},{OAUTH_BETA}"))
            }
        };
        let wait = match request.send_string(&payload) {
            Ok(response) => break response,
            Err(ureq::Error::Status(status, response)) => {
                let after = response.header("retry-after").and_then(|h| h.parse::<u64>().ok());
                let text = response.into_string().unwrap_or_default();
                let detail =
                    serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_string)).unwrap_or(text);
                if !matches!(status, 408 | 409 | 429 | 500..=599) || attempt >= MAX_ATTEMPTS {
                    return Err(Failure::Failed(friendly(status, &detail)));
                }
                after.unwrap_or(1 << attempt).min(30)
            }
            Err(ureq::Error::Transport(error)) => {
                if attempt >= MAX_ATTEMPTS {
                    return Err(Failure::Failed(format!("Sem conexão com a API: {error}")));
                }
                1 << attempt
            }
        };
        for _ in 0..wait * 10 {
            if stopped() {
                return Err(Failure::Stopped);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let mut assembler = Assembler::default();
    let mut finished = false;
    for line in std::io::BufReader::new(response.into_reader()).lines() {
        if stopped() {
            return Err(Failure::Stopped);
        }
        let line = line.map_err(|e| Failure::Failed(format!("A conexão com a API caiu: {e}")))?;
        let Some(data) = line.strip_prefix("data:") else { continue };
        let Ok(value) = serde_json::from_str::<Value>(data.trim_start()) else { continue };
        finished |= value["type"] == "message_stop";
        if let Some(text) = assembler.event(&value).map_err(Failure::Failed)? {
            on_text(&text);
        }
    }
    let turn = assembler.finish();
    // A stream that ends with no stop reason was cut off: its content can't be trusted as a turn.
    if !finished && turn.stop_reason.is_none() {
        return Err(Failure::Failed("A resposta da API foi interrompida.".to_string()));
    }
    Ok(turn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(events: &[Value]) -> Turn {
        let mut a = Assembler::default();
        for e in events {
            let _ = a.event(e);
        }
        a.finish()
    }

    fn block_start(index: u64, block: Value) -> Value {
        json!({"type": "content_block_start", "index": index, "content_block": block})
    }

    fn delta(index: u64, delta: Value) -> Value {
        json!({"type": "content_block_delta", "index": index, "delta": delta})
    }

    fn stop(index: u64) -> Value {
        json!({"type": "content_block_stop", "index": index})
    }

    #[test]
    fn the_request_has_what_the_current_api_expects() {
        let body = request_body("sistema", &[json!({"role": "user", "content": "oi"})]);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["cache_control"]["type"], "ephemeral");
        // These two were removed from this model and would be refused.
        assert!(body.get("temperature").is_none() && body["thinking"].get("budget_tokens").is_none());
        for tool in body["tools"].as_array().unwrap() {
            assert_eq!(tool["eager_input_streaming"], true, "{tool}");
            assert_eq!(tool["input_schema"]["additionalProperties"], false);
        }
    }

    #[test]
    fn text_thinking_and_tool_input_are_put_together() {
        let turn = run(&[
            json!({"type": "message_start", "message": {"usage": {"input_tokens": 10, "cache_read_input_tokens": 90}}}),
            block_start(0, json!({"type": "thinking", "thinking": "", "signature": ""})),
            delta(0, json!({"type": "thinking_delta", "thinking": "hm"})),
            delta(0, json!({"type": "signature_delta", "signature": "sig"})),
            stop(0),
            block_start(1, json!({"type": "text", "text": ""})),
            delta(1, json!({"type": "text_delta", "text": "o"})),
            delta(1, json!({"type": "text_delta", "text": "i"})),
            stop(1),
            block_start(2, json!({"type": "tool_use", "id": "t1", "name": "replace_text", "input": {}})),
            delta(2, json!({"type": "input_json_delta", "partial_json": "{\"old_text\":\"a"})),
            delta(2, json!({"type": "input_json_delta", "partial_json": "\",\"new_text\":\"é\"}"})),
            stop(2),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 7}}),
        ]);
        assert_eq!(turn.blocks[0]["thinking"], "hm");
        assert_eq!(turn.blocks[0]["signature"], "sig");
        assert_eq!(turn.blocks[1]["text"], "oi");
        assert_eq!(turn.blocks[2]["input"]["new_text"], "é");
        assert_eq!(turn.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(turn.usage, Usage { input: 10, output: 7, cache_read: 90, cache_write: 0 });
    }

    #[test]
    fn a_tool_with_no_parameters_gets_an_empty_input() {
        let turn = run(&[block_start(0, json!({"type": "tool_use", "id": "t", "name": "read_document"})), stop(0)]);
        assert_eq!(turn.blocks[0]["input"], json!({}));
        assert!(turn.invalid_inputs.is_empty());
    }

    #[test]
    fn invalid_tool_input_is_set_aside_not_passed_on() {
        for bad in ["{\"old_text\":", "[1,2]", "nope"] {
            let turn = run(&[
                block_start(0, json!({"type": "tool_use", "id": "t", "name": "replace_text"})),
                delta(0, json!({"type": "input_json_delta", "partial_json": bad})),
                stop(0),
            ]);
            assert_eq!(turn.invalid_inputs, [("t".to_string(), bad.to_string())]);
            assert_eq!(turn.blocks[0]["input"], json!({}), "the block stays valid to echo");
        }
    }

    #[test]
    fn a_stream_error_is_reported() {
        let mut a = Assembler::default();
        let err = a.event(&json!({"type": "error", "error": {"type": "overloaded_error", "message": "cheio"}}));
        assert_eq!(err, Err("overloaded_error: cheio".to_string()));
    }

    #[test]
    fn hostile_events_do_nothing() {
        for e in [
            json!({}),
            json!({"type": "content_block_delta", "index": 9, "delta": {"type": "text_delta", "text": "x"}}),
            json!({"type": "content_block_stop"}),
            json!(5),
        ] {
            let mut a = Assembler::default();
            assert_eq!(a.event(&e), Ok(None));
        }
    }

    #[test]
    fn a_turn_after_a_fallback_echoes_and_runs_only_what_may_be() {
        let turn = Turn {
            blocks: vec![
                json!({"type": "thinking", "thinking": "", "signature": "s"}),
                json!({"type": "text", "text": "antes"}),
                json!({"type": "tool_use", "id": "velho", "name": "append_text", "input": {}}),
                json!({"type": "fallback", "from": {"model": "a"}, "to": {"model": "b"}}),
                json!({"type": "thinking", "thinking": "", "signature": "z"}),
                json!({"type": "tool_use", "id": "novo", "name": "append_text", "input": {}}),
            ],
            ..Turn::default()
        };
        let echoed = turn.echo_blocks();
        let types: Vec<&str> = echoed.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["text", "thinking", "tool_use"]);
        let uses = turn.tool_uses();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0]["id"], "novo", "the abandoned call doesn't run");
    }
}
