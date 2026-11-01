//! Spell checking of a document's text: only what Typst's parser calls text, so code, math, labels
//! and raw blocks are never checked. The dictionary is parsed and consulted on a thread of its own,
//! and a panic there turns the feature off rather than taking the program down. `spellbook` is
//! young, and the words it is given come from whatever document is open.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use typst::syntax::{LinkedNode, SyntaxKind, parse};

const AFF: &str = include_str!("../assets/dict/pt_BR/pt_BR.aff");
const DIC: &str = include_str!("../assets/dict/pt_BR/pt_BR.dic");
/// Longer than any word: past this it's not worth a look.
const MAX_WORD_CHARS: usize = 64;
const MAX_SUGGESTIONS: usize = 6;
/// Names the dictionary can't know and every Pearl document may use.
const ALWAYS_KNOWN: [&str; 3] = ["Typst", "Pearl", "Rust"];

/// The words worth checking in a run of plain text, as byte ranges. Hyphens and apostrophes inside
/// a word keep it whole. Numbers, initials (`PDF`) and names in mixed case (`pdfTeX`) aren't words.
pub fn word_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = None;
    let mut end = 0;
    let mut flush = |start: &mut Option<usize>, end: usize| {
        if let Some(from) = start.take()
            && text.get(from..end).is_some_and(worth_checking)
        {
            out.push(from..end);
        }
    };
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let joins = matches!(c, '-' | '\'' | '’') && start.is_some() && chars.peek().is_some_and(|(_, next)| next.is_alphanumeric());
        if c.is_alphanumeric() || joins {
            start.get_or_insert(at);
            end = at + c.len_utf8();
        } else {
            flush(&mut start, end);
        }
    }
    flush(&mut start, end);
    out
}

fn worth_checking(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 || word.chars().count() > MAX_WORD_CHARS || word.chars().any(|c| c.is_numeric()) {
        return false;
    }
    let all_caps = word.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
    let mixed = word.chars().skip(1).zip(word.chars()).any(|(c, before)| c.is_uppercase() && before.is_lowercase());
    !all_caps && !mixed
}

fn collect(source: &str, node: &LinkedNode, out: &mut Vec<Range<usize>>) {
    for child in node.children() {
        if child.kind() == SyntaxKind::Text {
            let offset = child.offset();
            out.extend(word_ranges(source.get(child.range()).unwrap_or_default()).into_iter().map(|r| r.start + offset..r.end + offset));
        } else if !matches!(child.kind(), SyntaxKind::Raw | SyntaxKind::Equation | SyntaxKind::Label | SyntaxKind::Ref | SyntaxKind::Link) {
            collect(source, &child, out);
        }
    }
}

/// The words of a document's text, as byte ranges.
pub fn document_words(source: &str) -> Vec<Range<usize>> {
    let root = parse(source);
    let mut out = Vec::new();
    collect(source, &LinkedNode::new(&root), &mut out);
    out
}

/// The word at this byte of the source, if it's one that gets checked.
pub fn word_at(source: &str, byte: usize) -> Option<Range<usize>> {
    document_words(source).into_iter().find(|r| r.start <= byte && byte <= r.end)
}

fn find_language(source: &str, node: &LinkedNode) -> Option<String> {
    let text_of = |n: &LinkedNode| source.get(n.range()).unwrap_or_default().to_string();
    if node.kind() == SyntaxKind::SetRule {
        let mut in_text = false;
        for child in node.children() {
            if child.kind() == SyntaxKind::Ident && text_of(&child) == "text" {
                in_text = true;
            }
            if in_text && child.kind() == SyntaxKind::Args {
                for arg in child.children().filter(|a| a.kind() == SyntaxKind::Named) {
                    let mut parts = arg.children();
                    if parts.next().is_some_and(|name| text_of(&name) == "lang")
                        && let Some(value) = parts.find(|p| p.kind() == SyntaxKind::Str)
                    {
                        return Some(text_of(&value).trim_matches('"').to_string());
                    }
                }
            }
        }
    }
    node.children().find_map(|c| find_language(source, &c))
}

/// Whether a document is in a language the dictionary knows: it says Portuguese in its first
/// `#set text(lang: …)`, or says nothing.
pub fn in_known_language(source: &str) -> bool {
    let root = parse(source);
    find_language(source, &LinkedNode::new(&root)).is_none_or(|lang| lang.eq_ignore_ascii_case("pt"))
}

/// The dictionary, with the small allowances over it.
pub struct Checker {
    dictionary: spellbook::Dictionary,
}

impl Checker {
    pub fn new() -> Result<Self, String> {
        let dictionary =
            spellbook::Dictionary::new(AFF.trim_start_matches('\u{feff}'), DIC.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        Ok(Self { dictionary })
    }

    pub fn check(&self, word: &str) -> bool {
        ALWAYS_KNOWN.contains(&word) || self.dictionary.check(word)
    }

    /// Suggestions without the dictionary's stray splits: `e xemlo` for `exemlo` is no help, while
    /// `com certeza` for `concerteza` is.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut found = Vec::new();
        self.dictionary.suggest(word, &mut found);
        found.retain(|s| {
            let mut parts = s.split([' ', '-']);
            !s.contains([' ', '-']) || word.contains('-') || parts.all(|p| p.chars().count() >= 2 && self.dictionary.check(p))
        });
        found.truncate(MAX_SUGGESTIONS);
        found
    }
}

enum Job {
    Check(Vec<String>),
    Suggest(String),
}

pub enum Event {
    /// Each word asked about, and whether it's right.
    Checked(Vec<(String, bool)>),
    Suggested(String, Vec<String>),
    /// The checker is gone, for this reason.
    Failed(String),
}

/// The thread with the dictionary.
pub struct Speller {
    jobs: Sender<Job>,
    pub events: Receiver<Event>,
    /// Who to wake when there's an answer.
    pub repaint: Arc<OnceLock<egui::Context>>,
}

impl Default for Speller {
    fn default() -> Self {
        Self::start()
    }
}

impl Speller {
    pub fn start() -> Self {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, events) = channel();
        let repaint: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let wake = Arc::clone(&repaint);
        let _ = std::thread::Builder::new().name("spell".into()).spawn(move || {
            let say = |event| {
                let _ = outbox.send(event);
                if let Some(ctx) = wake.get() {
                    ctx.request_repaint();
                }
            };
            let run = std::panic::AssertUnwindSafe(|| {
                let checker = match Checker::new() {
                    Ok(checker) => checker,
                    Err(e) => return say(Event::Failed(e)),
                };
                for job in inbox {
                    match job {
                        Job::Check(words) => say(Event::Checked(words.into_iter().map(|w| (checker.check(&w), w)).map(|(ok, w)| (w, ok)).collect())),
                        Job::Suggest(word) => {
                            let found = checker.suggest(&word);
                            say(Event::Suggested(word, found));
                        }
                    }
                }
            });
            if std::panic::catch_unwind(run).is_err() {
                say(Event::Failed("panic".to_string()));
            }
        });
        Self { jobs, events, repaint }
    }

    pub fn check(&self, words: Vec<String>) {
        let _ = self.jobs.send(Job::Check(words));
    }

    pub fn suggest(&self, word: &str) {
        let _ = self.jobs.send(Job::Suggest(word.to_string()));
    }
}

/// A personal dictionary from its file: a word per line.
pub fn parse_personal(text: &str) -> HashSet<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(source: &str) -> Vec<&str> {
        document_words(source).into_iter().map(|r| &source[r]).collect()
    }

    #[test]
    fn only_the_documents_text_is_checked() {
        let source = "#set text(lang: \"pt\")\n= Títulu\nTexto *forte* _leve_ $ errro $ `errro` <errro> @errro https://errro.org\n#figure(table([errro]), caption: [legenda])\n// errro\n#let errro = \"errro\"\n";
        let found = words(source);
        assert_eq!(found.iter().filter(|w| **w == "errro").count(), 1, "only in the table's content: {found:?}");
        for word in ["Títulu", "Texto", "forte", "leve", "legenda"] {
            assert!(found.contains(&word), "{word}: {found:?}");
        }
    }

    #[test]
    fn numbers_initials_and_mixed_case_are_not_words() {
        let text = "PDF 3cm pdfTeX H2O a guarda-chuva d’água fim- -x";
        let found: Vec<&str> = word_ranges(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["guarda-chuva", "d’água", "fim"]);
    }

    #[test]
    fn the_word_under_a_byte() {
        let source = "uma palavra\n";
        assert_eq!(word_at(source, 6), Some(4..11));
        assert_eq!(word_at(source, 11), Some(4..11), "the caret just after it");
        assert_eq!(word_at("#let x = 1", 2), None);
    }

    #[test]
    fn the_language_the_document_declares_decides() {
        assert!(in_known_language("Texto"));
        assert!(in_known_language("#set text(lang: \"pt\")\nTexto"));
        assert!(in_known_language("#set text(size: 11pt, lang: \"pt\", region: \"br\")\nTexto"));
        assert!(!in_known_language("#set text(lang: \"en\")\nText"));
        assert!(in_known_language("#set page(margin: 1cm)\n#set text(size: 11pt)\nTexto"));
    }

    #[test]
    fn portuguese_is_right_and_wrong_where_it_should_be() {
        let c = Checker::new().unwrap();
        for good in ["página", "fazê-lo", "disse-lhe", "guarda-chuva", "Bem-vindo", "São", "Typst", "corações"] {
            assert!(c.check(good), "{good}");
        }
        for bad in ["exemlo", "progamação", "cachoro", "fazer-lo", "xyzzy"] {
            assert!(!c.check(bad), "{bad}");
        }
    }

    #[test]
    fn suggestions_help_and_stray_splits_are_gone() {
        let c = Checker::new().unwrap();
        assert!(c.suggest("exemlo").contains(&"exemplo".to_string()));
        assert!(c.suggest("cachoro").contains(&"cachorro".to_string()));
        assert!(c.suggest("progamação").contains(&"programação".to_string()));
        assert!(c.suggest("concerteza").contains(&"com certeza".to_string()));
        let all = [c.suggest("exemlo"), c.suggest("cachoro")].concat();
        assert!(!all.iter().any(|s| s == "e xemlo" || s == "ex emlo" || s == "cacho-o"), "{all:?}");
    }

    #[test]
    fn what_ships_with_the_program_has_no_errors() {
        let c = Checker::new().unwrap();
        let sources: Vec<&str> = std::iter::once(crate::sample::SAMPLE).chain(crate::templates::TEMPLATES.iter().map(|t| t.source)).collect();
        let wrong: Vec<String> =
            sources.iter().flat_map(|s| words(s).into_iter().filter(|w| !c.check(w)).map(str::to_string).collect::<Vec<_>>()).collect();
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    #[test]
    fn the_thread_answers_and_keeps_answering() {
        let s = Speller::start();
        s.check(vec!["casa".to_string(), "caza0".to_string()]);
        let timeout = std::time::Duration::from_secs(60);
        let Event::Checked(answer) = s.events.recv_timeout(timeout).unwrap() else { panic!("not an answer") };
        assert_eq!(answer, [("casa".to_string(), true), ("caza0".to_string(), false)]);
        s.suggest("cachoro");
        let Event::Suggested(word, found) = s.events.recv_timeout(timeout).unwrap() else { panic!("not an answer") };
        assert_eq!(word, "cachoro");
        assert!(!found.is_empty());
    }

    #[test]
    fn a_personal_dictionary_is_a_word_per_line() {
        assert_eq!(parse_personal("Bertuzzi\n\n  Kühlsen \n").len(), 2);
    }
}
