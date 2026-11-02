//! Comments on the text: threads tied to a passage. They aren't part of the source, so the document and
//! its PDF stay as they are. They live as flat entries, one for each fact, which a session shares in its
//! document so that two people replying at once both stay: each only ever adds its own entry.
//!
//! An entry from a peer or a file is hostile input, read within limits: what doesn't parse doesn't count.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use serde_json::{Value, json};

use crate::collab::doc::diff;
use crate::edit;

const MAX_THREADS: usize = 1000;
const MAX_MESSAGES: usize = 500;
const MAX_TEXT: usize = 10_000;
const MAX_QUOTE: usize = 2000;
const MAX_AUTHOR: usize = 40;
const MAX_ID: usize = 32;
/// Letters on each side kept to tell one occurrence of a passage from another.
const CONTEXT: usize = 40;
/// Occurrences of a passage looked at, in a document that repeats it a lot.
const MAX_OCCURRENCES: usize = 1000;
/// Passages drawn in the editor.
const MAX_DRAWN: usize = 200;
pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_ENTRY: usize = 3 * MAX_TEXT;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub author: String,
    pub stamp: u64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub quote: String,
    before: String,
    after: String,
    /// The byte it was at when written, to choose among the same words elsewhere.
    hint: usize,
    pub stamp: u64,
    pub resolved: bool,
    pub messages: Vec<Message>,
}

fn clean(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

fn good_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn string_of(value: &Value, key: &str, most: usize) -> String {
    value.get(key).and_then(Value::as_str).map_or_else(String::new, |s| clean(s, most))
}

fn number_of(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// A new id, unlike those of the others in a session.
fn new_id(now: u64) -> String {
    format!("{:016x}", getrandom::u64().unwrap_or(now))
}

fn head_entry(thread: &Thread) -> String {
    json!({"quote": thread.quote, "before": thread.before, "after": thread.after, "hint": thread.hint, "stamp": thread.stamp}).to_string()
}

fn message_entry(message: &Message) -> String {
    json!({"author": message.author, "stamp": message.stamp, "text": message.text}).to_string()
}

/// The threads in a set of entries, oldest first. What doesn't parse, or has no words, doesn't count.
fn threads_of(entries: &BTreeMap<String, String>) -> Vec<Thread> {
    let is_on = |prefix: &str, id: &str| entries.get(&format!("{prefix}/{id}")).is_some_and(|v| v == "1");
    let mut threads: BTreeMap<String, Thread> = BTreeMap::new();
    for (key, value) in entries {
        let Some(id) = key.strip_prefix("t/").filter(|id| good_id(id)) else { continue };
        let Ok(head) = serde_json::from_str::<Value>(value) else { continue };
        let quote = string_of(&head, "quote", MAX_QUOTE);
        if threads.len() >= MAX_THREADS || quote.is_empty() || is_on("d", id) {
            continue;
        }
        let thread = Thread {
            id: id.to_string(),
            quote,
            before: string_of(&head, "before", CONTEXT),
            after: string_of(&head, "after", CONTEXT),
            hint: usize::try_from(number_of(&head, "hint")).unwrap_or(0),
            stamp: number_of(&head, "stamp"),
            resolved: is_on("r", id),
            messages: Vec::new(),
        };
        threads.insert(id.to_string(), thread);
    }
    for (key, value) in entries {
        let Some((id, message)) = key.strip_prefix("m/").and_then(|rest| rest.split_once('/')) else { continue };
        let Some(thread) = threads.get_mut(id) else { continue };
        let Ok(body) = serde_json::from_str::<Value>(value) else { continue };
        let text = string_of(&body, "text", MAX_TEXT);
        if good_id(message) && !text.trim().is_empty() && thread.messages.len() < MAX_MESSAGES {
            let author = string_of(&body, "author", MAX_AUTHOR);
            thread.messages.push(Message { id: message.to_string(), author, stamp: number_of(&body, "stamp"), text });
        }
    }
    let mut list: Vec<Thread> = threads.into_values().filter(|t| !t.messages.is_empty()).collect();
    for thread in &mut list {
        thread.messages.sort_by(|a, b| (a.stamp, &a.id).cmp(&(b.stamp, &b.id)));
    }
    list.sort_by(|a, b| (a.stamp, &a.id).cmp(&(b.stamp, &b.id)));
    list
}

/// The letters around a range, on character boundaries.
fn context_of(text: &str, range: &Range<usize>) -> (String, String) {
    let before: Vec<char> = text.get(..range.start).unwrap_or_default().chars().rev().take(CONTEXT).collect();
    let after = text.get(range.end..).unwrap_or_default().chars().take(CONTEXT).collect();
    (before.into_iter().rev().collect(), after)
}

/// Where a thread's passage is: the occurrence with most of what was around it, then the nearest.
fn locate(text: &str, thread: &Thread) -> Option<Range<usize>> {
    let score = |at: usize| {
        let range = at..at + thread.quote.len();
        let (before, after) = context_of(text, &range);
        let same_before = before.chars().rev().zip(thread.before.chars().rev()).take_while(|(a, b)| a == b).count();
        let same_after = after.chars().zip(thread.after.chars()).take_while(|(a, b)| a == b).count();
        (same_before + same_after, std::cmp::Reverse(at.abs_diff(thread.hint)))
    };
    let best = text.match_indices(thread.quote.as_str()).take(MAX_OCCURRENCES).map(|(at, _)| at).max_by_key(|at| score(*at))?;
    Some(best..best + thread.quote.len())
}

/// Where a range is after a change, or nothing if none of it is left.
fn followed(range: &Range<usize>, start: usize, end: usize, added: usize) -> Option<Range<usize>> {
    let moved = |at: usize| at - (end - start) + added;
    let from = if range.start >= end {
        moved(range.start)
    } else if range.start > start {
        start + added
    } else {
        range.start
    };
    // Typed right after the passage, it isn't part of it.
    let to = if range.end >= end && !(start == end && range.end == end) {
        moved(range.end)
    } else if range.end > start {
        start
    } else {
        range.end
    };
    (from < to).then_some(from..to)
}

/// All the threads of a document, and where each passage is in its text now.
#[derive(Default)]
pub struct Book {
    entries: BTreeMap<String, String>,
    threads: Vec<Thread>,
    places: HashMap<String, Range<usize>>,
    /// The open passages, as char ranges, for drawing.
    drawn: Vec<(usize, usize)>,
    known: String,
}

impl Book {
    pub fn threads(&self) -> &[Thread] {
        &self.threads
    }

    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }

    pub fn known_text(&self) -> &str {
        &self.known
    }

    pub fn drawn(&self) -> &[(usize, usize)] {
        &self.drawn
    }

    /// Where a thread's passage is, if it's still in the text.
    pub fn place(&self, id: &str) -> Option<Range<usize>> {
        self.places.get(id).cloned()
    }

    pub fn open_count(&self) -> usize {
        self.threads.iter().filter(|t| !t.resolved).count()
    }

    /// Reads the threads again, keeping the places known and looking for those that aren't.
    fn rebuild(&mut self, text: &str) {
        self.threads = threads_of(&self.entries);
        let ids: Vec<&str> = self.threads.iter().map(|t| t.id.as_str()).collect();
        self.places.retain(|id, _| ids.contains(&id.as_str()));
        self.known.clear();
        self.known.push_str(text);
        self.relocate();
    }

    /// Looks again for what isn't placed, which an undo may have brought back, and for drawing.
    fn relocate(&mut self) {
        for thread in &self.threads {
            if !self.places.contains_key(&thread.id)
                && let Some(found) = locate(&self.known, thread)
            {
                self.places.insert(thread.id.clone(), found);
            }
        }
        let open = self.threads.iter().filter(|t| !t.resolved).filter_map(|t| self.places.get(&t.id)).take(MAX_DRAWN);
        self.drawn = open.map(|range| (edit::char_of(&self.known, range.start), edit::char_of(&self.known, range.end))).collect();
    }

    /// Takes in another set of entries, as a file or a session has them.
    pub fn replace_entries(&mut self, entries: BTreeMap<String, String>, text: &str) {
        self.entries = entries;
        self.rebuild(text);
    }

    /// Follows the text to what it says now. True if it had changed.
    pub fn follow(&mut self, text: &str) -> bool {
        if self.known == text {
            return false;
        }
        let (start, end, inserted) = diff(&self.known, text);
        let added = inserted.len();
        let places = std::mem::take(&mut self.places);
        self.places = places
            .into_iter()
            .filter_map(|(id, range)| {
                let new = followed(&range, start, end, added)?;
                text.get(new.clone()).is_some().then_some((id, new))
            })
            .collect();
        self.known.clear();
        self.known.push_str(text);
        self.relocate();
        true
    }

    fn set(&mut self, new: Vec<(String, String)>) -> Vec<(String, String)> {
        for (key, value) in &new {
            self.entries.insert(key.clone(), value.clone());
        }
        let text = std::mem::take(&mut self.known);
        self.rebuild(&text);
        new
    }

    /// Starts a thread on a byte range of the text. The entries it made, to share.
    pub fn start(&mut self, text: &str, range: Range<usize>, author: &str, body: &str, now: u64) -> Vec<(String, String)> {
        let Some(quote) = text.get(range.clone()).filter(|q| !q.trim().is_empty() && !body.trim().is_empty()) else { return Vec::new() };
        // A long passage is held by its beginning, which is what's drawn and looked for.
        let quote = clean(quote, MAX_QUOTE);
        let range = range.start..range.start + quote.len();
        let (before, after) = context_of(text, &range);
        let id = new_id(now);
        let head = Thread { id: id.clone(), quote, before, after, hint: range.start, stamp: now, resolved: false, messages: Vec::new() };
        let message = Message { id: new_id(now), author: clean(author, MAX_AUTHOR), stamp: now, text: clean(body, MAX_TEXT) };
        self.places.insert(id.clone(), range);
        self.set(vec![(format!("t/{id}"), head_entry(&head)), (format!("m/{id}/{}", message.id), message_entry(&message))])
    }

    pub fn reply(&mut self, thread: &str, author: &str, body: &str, now: u64) -> Vec<(String, String)> {
        if body.trim().is_empty() || !self.threads.iter().any(|t| t.id == thread) {
            return Vec::new();
        }
        let message = Message { id: new_id(now), author: clean(author, MAX_AUTHOR), stamp: now, text: clean(body, MAX_TEXT) };
        self.set(vec![(format!("m/{thread}/{}", message.id), message_entry(&message))])
    }

    pub fn set_resolved(&mut self, thread: &str, resolved: bool) -> Vec<(String, String)> {
        self.set(vec![(format!("r/{thread}"), if resolved { "1" } else { "0" }.to_string())])
    }

    pub fn delete(&mut self, thread: &str) -> Vec<(String, String)> {
        self.set(vec![(format!("d/{thread}"), "1".to_string())])
    }

    /// The threads as a file holds them, with their passages as they are now.
    pub fn to_file(&self) -> String {
        let threads: Vec<Value> = self
            .threads
            .iter()
            .map(|thread| {
                let place = self.places.get(&thread.id);
                let live = place.and_then(|range| self.known.get(range.clone()));
                let (before, after) = place.map_or_else(|| (thread.before.clone(), thread.after.clone()), |range| context_of(&self.known, range));
                let messages: Vec<Value> =
                    thread.messages.iter().map(|m| json!({"id": m.id, "author": m.author, "stamp": m.stamp, "text": m.text})).collect();
                json!({
                    "id": thread.id, "quote": live.unwrap_or(&thread.quote), "before": before, "after": after,
                    "hint": place.map_or(thread.hint, |range| range.start), "stamp": thread.stamp,
                    "resolved": thread.resolved, "messages": messages,
                })
            })
            .collect();
        serde_json::to_string_pretty(&json!({"version": 1, "threads": threads})).unwrap_or_default()
    }

    /// The entries a file holds, or nothing if it isn't one of these.
    pub fn entries_from_file(text: &str) -> Option<BTreeMap<String, String>> {
        let value: Value = serde_json::from_str(text).ok()?;
        let threads = value.get("threads")?.as_array()?;
        let mut entries = BTreeMap::new();
        for thread in threads.iter().take(MAX_THREADS) {
            let id = string_of(thread, "id", MAX_ID + 1);
            if !good_id(&id) {
                continue;
            }
            let head = Thread {
                id: id.clone(),
                quote: string_of(thread, "quote", MAX_QUOTE),
                before: string_of(thread, "before", CONTEXT),
                after: string_of(thread, "after", CONTEXT),
                hint: usize::try_from(number_of(thread, "hint")).unwrap_or(0),
                stamp: number_of(thread, "stamp"),
                resolved: false,
                messages: Vec::new(),
            };
            entries.insert(format!("t/{id}"), head_entry(&head));
            if thread.get("resolved").and_then(Value::as_bool).unwrap_or(false) {
                entries.insert(format!("r/{id}"), "1".to_string());
            }
            for body in thread.get("messages").and_then(Value::as_array).map_or(&[][..], Vec::as_slice).iter().take(MAX_MESSAGES) {
                let message = string_of(body, "id", MAX_ID + 1);
                if good_id(&message) {
                    let parsed = Message {
                        id: message.clone(),
                        author: string_of(body, "author", MAX_AUTHOR),
                        stamp: number_of(body, "stamp"),
                        text: string_of(body, "text", MAX_TEXT),
                    };
                    entries.insert(format!("m/{id}/{message}"), message_entry(&parsed));
                }
            }
        }
        Some(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "Um começo. O trecho comentado aqui. Um fim.\n";

    fn range_of(text: &str, part: &str) -> Range<usize> {
        let at = text.find(part).unwrap();
        at..at + part.len()
    }

    fn with_thread(text: &str, part: &str) -> (Book, String) {
        let mut book = Book::default();
        book.follow(text);
        book.start(text, range_of(text, part), "Ana", "Reveja isto.", 10);
        let id = book.threads()[0].id.clone();
        (book, id)
    }

    fn passage(book: &Book, id: &str, text: &str) -> Option<String> {
        book.place(id).and_then(|range| text.get(range)).map(str::to_string)
    }

    #[test]
    fn a_thread_keeps_its_place_as_the_text_changes_around_it() {
        let (mut book, id) = with_thread(TEXT, "trecho comentado");
        assert_eq!(passage(&book, &id, TEXT).as_deref(), Some("trecho comentado"));

        let before = format!("Algo novo no início. {TEXT}");
        book.follow(&before);
        assert_eq!(passage(&book, &id, &before).as_deref(), Some("trecho comentado"));

        let after = before.replace("Um fim", "Um fim bem maior");
        book.follow(&after);
        assert_eq!(passage(&book, &id, &after).as_deref(), Some("trecho comentado"));
    }

    #[test]
    fn typing_at_its_edges_stays_out_and_inside_it_goes_along() {
        let (mut book, id) = with_thread(TEXT, "trecho comentado");
        let at_end = TEXT.replace("comentado", "comentado!");
        book.follow(&at_end);
        assert_eq!(passage(&book, &id, &at_end).as_deref(), Some("trecho comentado"));
        let at_start = at_end.replace("trecho", "#trecho");
        book.follow(&at_start);
        assert_eq!(passage(&book, &id, &at_start).as_deref(), Some("trecho comentado"));
        let inside = at_start.replace("trecho comentado", "trecho já comentado");
        book.follow(&inside);
        assert_eq!(passage(&book, &id, &inside).as_deref(), Some("trecho já comentado"), "a fix inside doesn't lose it");
    }

    #[test]
    fn a_passage_deleted_loses_its_place_and_finds_it_again_on_undo() {
        let (mut book, id) = with_thread(TEXT, "trecho comentado");
        let gone = TEXT.replace("trecho comentado ", "");
        book.follow(&gone);
        assert_eq!(book.place(&id), None);
        assert_eq!(book.threads().len(), 1, "the thread stays, with the words it was on");
        assert_eq!(book.threads()[0].quote, "trecho comentado");
        book.follow(TEXT);
        assert_eq!(passage(&book, &id, TEXT).as_deref(), Some("trecho comentado"));
    }

    #[test]
    fn the_same_words_elsewhere_are_told_apart_by_what_surrounds_them() {
        let text = "alvo um. A frase do alvo certo. E alvo dois.\n";
        let at = text.find("do alvo").unwrap() + 3;
        let mut book = Book::default();
        book.follow(text);
        book.start(text, at..at + 4, "Ana", "Este.", 1);
        let file = book.to_file();
        let reopened = format!("Mais texto no início, e alvo também. {text}");
        let mut other = Book::default();
        other.replace_entries(Book::entries_from_file(&file).unwrap(), &reopened);
        let id = other.threads()[0].id.clone();
        let place = other.place(&id).unwrap();
        assert!(reopened.get(..place.start).unwrap().ends_with("A frase do "), "{place:?}");
    }

    #[test]
    fn replies_resolving_and_deleting_are_entries_that_merge() {
        let (mut book, id) = with_thread(TEXT, "trecho");
        let mut peer = Book::default();
        peer.replace_entries(book.entries().clone(), TEXT);

        // Both reply at once: each adds an entry of its own, and neither is lost.
        let mine = book.reply(&id, "Ana", "Minha resposta.", 20);
        let theirs = peer.reply(&id, "Bia", "A dela.", 21);
        for (key, value) in theirs {
            book.entries.insert(key, value);
        }
        for (key, value) in mine {
            peer.entries.insert(key, value);
        }
        book.rebuild(TEXT);
        peer.rebuild(TEXT);
        assert_eq!(book.threads(), peer.threads());
        let said: Vec<&str> = book.threads()[0].messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(said, ["Reveja isto.", "Minha resposta.", "A dela."]);

        book.set_resolved(&id, true);
        assert_eq!(book.open_count(), 0);
        assert!(book.drawn().is_empty(), "a resolved one isn't drawn");
        book.set_resolved(&id, false);
        assert_eq!(book.open_count(), 1);
        assert_eq!(book.drawn().len(), 1);
        book.delete(&id);
        assert!(book.threads().is_empty());
    }

    #[test]
    fn empty_comments_and_blank_passages_start_nothing() {
        let mut book = Book::default();
        book.follow(TEXT);
        assert!(book.start(TEXT, range_of(TEXT, "trecho"), "Ana", "   ", 1).is_empty());
        assert!(book.start(TEXT, 0..0, "Ana", "Algo", 1).is_empty());
        assert!(book.start("  \n ", 0..3, "Ana", "Algo", 1).is_empty());
        assert!(book.start(TEXT, 9000..9100, "Ana", "Algo", 1).is_empty());
        assert!(book.threads().is_empty());
        assert!(book.reply("nao-existe", "Ana", "Algo", 1).is_empty());
    }

    #[test]
    fn a_file_brings_it_all_back() {
        let (mut book, id) = with_thread(TEXT, "trecho comentado");
        book.reply(&id, "Bia", "Concordo.", 20);
        book.set_resolved(&id, true);
        let mut again = Book::default();
        again.replace_entries(Book::entries_from_file(&book.to_file()).unwrap(), TEXT);
        assert_eq!(again.threads().len(), 1);
        let (ours, theirs) = (&book.threads()[0], &again.threads()[0]);
        assert_eq!(ours.messages, theirs.messages);
        assert_eq!((ours.quote.as_str(), ours.resolved), (theirs.quote.as_str(), theirs.resolved));
    }

    #[test]
    fn hostile_entries_and_files_are_survived() {
        for text in ["", "{", "[]", "{\"threads\": 3}", "{\"threads\": [1, {}, {\"id\": \"../x\"}, {\"id\": \"ab\", \"messages\": 3}]}"] {
            let entries = Book::entries_from_file(text).unwrap_or_default();
            let mut book = Book::default();
            book.replace_entries(entries, TEXT);
            assert!(book.threads().is_empty(), "{text}");
        }
        let mut entries = BTreeMap::new();
        for (key, value) in [
            ("t/zz", "{\"quote\":\"x\"}"),
            ("t/ab", "não é json"),
            ("t/cd", "{\"quote\":\"\"}"),
            ("t/ef", "{\"quote\":\"trecho\",\"hint\":1e300,\"stamp\":-4}"),
            ("m/ef/zz", "{\"text\":\"id ruim\"}"),
            ("m/ef/01", "{\"text\":5}"),
            ("m/ef/02", "{\"text\":\"Sobrevive.\",\"author\":7}"),
            ("m/semthread/03", "{\"text\":\"solta\"}"),
            ("/", ""),
        ] {
            entries.insert(key.to_string(), value.to_string());
        }
        let mut book = Book::default();
        book.replace_entries(entries, TEXT);
        assert_eq!(book.threads().len(), 1);
        assert_eq!(book.threads()[0].messages[0].text, "Sobrevive.");
        assert_eq!(book.threads()[0].messages[0].author, "");

        // A place from a stale text is never used to cut inside a letter.
        let (mut book, id) = with_thread("ação ação", "ação");
        book.follow("ç");
        book.follow("");
        assert_eq!(book.place(&id), None);
    }

    #[test]
    fn a_comment_with_no_message_left_is_no_thread() {
        let mut entries = BTreeMap::new();
        entries.insert("t/ab".to_string(), "{\"quote\":\"trecho\"}".to_string());
        let mut book = Book::default();
        book.replace_entries(entries, TEXT);
        assert!(book.threads().is_empty());
    }
}
