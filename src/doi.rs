//! Asking the outside world about references: a DOI's entry from doi.org, and whether an entry can be
//! found there or at Crossref. Nothing here runs unless the person asks. Only DOIs and titles leave
//! the machine, and every reply is hostile input, read within a limit.

use std::io::Read;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use crate::bib::Entry;

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REPLY: u64 = 2 * 1024 * 1024;
/// More than this in one check is more than the services should be asked at once.
pub const MAX_CHECKED: usize = 500;
const PAUSE: Duration = Duration::from_millis(100);
/// How many of Crossref's matches for a title are compared with it.
const ROWS: u32 = 10;

/// Where to ask. Tests point this at a pretend server.
#[derive(Clone, Debug)]
pub struct Servers {
    pub doi: String,
    pub crossref: String,
}

impl Default for Servers {
    fn default() -> Self {
        Self { doi: "https://doi.org".to_string(), crossref: "https://api.crossref.org".to_string() }
    }
}

/// What a check found out about one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Found,
    /// The DOI exists, under this other title.
    OtherTitle(String),
    NotFound,
    /// The question couldn't be asked or answered, which says nothing about the entry.
    Failed,
    /// It has neither a DOI nor a title to ask about.
    NothingToAsk,
}

/// The DOI in what was pasted: bare, as `doi:…` or as an address.
pub fn extract(input: &str) -> Option<String> {
    let start = input.find("10.")?;
    let doi = input.get(start..)?.split_whitespace().next()?;
    let doi = doi.trim_end_matches(['.', ',', ';', ')', ']', '}', '"', '\'', '>']);
    let (prefix, suffix) = doi.split_once('/')?;
    let digits = prefix.strip_prefix("10.")?;
    let valid = (4..=9).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) && !suffix.is_empty();
    valid.then(|| doi.to_string())
}

fn encode(text: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') || (keep_slash && byte == b'/') {
            out.push(char::from(byte));
        } else {
            out += &format!("%{byte:02X}");
        }
    }
    out
}

enum Reply {
    Body(String),
    Missing,
    Failed,
}

fn ask(url: &str, accept: &str) -> Reply {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let request = agent.get(url).set("Accept", accept).set("User-Agent", concat!("pearl/", env!("CARGO_PKG_VERSION")));
    match request.call() {
        Ok(response) => {
            let mut body = String::new();
            match response.into_reader().take(MAX_REPLY).read_to_string(&mut body) {
                Ok(_) => Reply::Body(body),
                Err(_) => Reply::Failed,
            }
        }
        Err(ureq::Error::Status(404, _)) => Reply::Missing,
        Err(_) => Reply::Failed,
    }
}

/// The BibTeX of a DOI's entry. A failure is the id of the message that explains it.
pub fn fetch_bibtex(servers: &Servers, doi: &str) -> Result<String, &'static str> {
    match ask(&format!("{}/{}", servers.doi, encode(doi, true)), "application/x-bibtex") {
        Reply::Body(body) => Ok(body),
        Reply::Missing => Err("the-doi-was-not-found"),
        Reply::Failed => Err("the-doi-could-not-be-looked-up"),
    }
}

/// Runs a job on a thread of its own and wakes the window with its result.
pub fn in_background<T: Send + 'static>(ctx: &egui::Context, job: impl FnOnce() -> T + Send + 'static) -> Option<Receiver<T>> {
    let (tx, rx) = channel();
    let waker = ctx.clone();
    let started = std::thread::Builder::new().name("references-online".into()).spawn(move || {
        let _ = tx.send(job());
        waker.request_repaint();
    });
    started.ok().map(|_| rx)
}

/// Looks up each entry in turn, sending what it finds. Ends early when no one is listening any more.
pub fn check_all(ctx: &egui::Context, servers: Servers, entries: Vec<Entry>) -> Option<Receiver<(String, Verdict)>> {
    let (tx, rx) = channel();
    let waker = ctx.clone();
    let started = std::thread::Builder::new().name("references-check".into()).spawn(move || {
        for entry in entries.into_iter().take(MAX_CHECKED) {
            let verdict = check(&servers, &entry);
            if tx.send((entry.identity(), verdict)).is_err() {
                return;
            }
            waker.request_repaint();
            std::thread::sleep(PAUSE);
        }
    });
    started.ok().map(|_| rx)
}

fn check(servers: &Servers, entry: &Entry) -> Verdict {
    if let Some(doi) = entry.doi.as_deref().and_then(extract) {
        return match ask(&format!("{}/{}", servers.doi, encode(&doi, true)), "application/vnd.citationstyles.csl+json") {
            Reply::Body(body) => match title_in_csl(&body) {
                Some(title) if !entry.title.is_empty() && !same_title(&entry.title, &title) => Verdict::OtherTitle(title),
                _ => Verdict::Found,
            },
            Reply::Missing => Verdict::NotFound,
            Reply::Failed => Verdict::Failed,
        };
    }
    if entry.title.is_empty() {
        return Verdict::NothingToAsk;
    }
    let url = format!("{}/works?rows={ROWS}&select=DOI,title&query.bibliographic={}", servers.crossref, encode(&entry.title, false));
    match ask(&url, "application/json") {
        Reply::Body(body) => match titles_in_crossref(&body) {
            Some(titles) if titles.iter().any(|t| same_title(&entry.title, t)) => Verdict::Found,
            Some(_) => Verdict::NotFound,
            None => Verdict::Failed,
        },
        Reply::Missing | Reply::Failed => Verdict::Failed,
    }
}

fn title_in_csl(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let title = value.get("title")?;
    title.as_str().or_else(|| title.get(0)?.as_str()).map(str::to_string)
}

fn titles_in_crossref(body: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let items = value.get("message")?.get("items")?.as_array()?;
    Some(items.iter().filter_map(|item| item.get("title")?.get(0)?.as_str().map(str::to_string)).collect())
}

/// Whether two titles are the same work's: the shorter one's words nearly all in the longer, which
/// may add a subtitle, and not much longer than that. Services write marks and accents differently.
pub fn same_title(a: &str, b: &str) -> bool {
    let (a, b) = (title_words(a), title_words(b));
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if short.is_empty() {
        return false;
    }
    if short.len() == 1 || short.len() * 2 < long.len() {
        return short == long;
    }
    let shared = short.iter().filter(|word| long.contains(word)).count();
    shared * 100 >= short.len() * 85
}

fn title_words(title: &str) -> Vec<String> {
    let plain = title.replace("&amp;", " ").replace("&lt;", " ").replace("&gt;", " ");
    crate::bib::fold(&plain).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the real services: `cargo test real_services -- --ignored`. It sends one public DOI and
    /// one public title, and needs the internet, so it isn't among the gates.
    #[test]
    #[ignore = "uses the internet"]
    fn real_services() {
        let servers = Servers::default();
        let bibtex = fetch_bibtex(&servers, "10.1038/nature14539").unwrap();
        let entries = crate::bib::parse_bib(&bibtex).unwrap();
        assert_eq!(entries[0].title, "Deep learning");
        assert_eq!(check(&servers, &entries[0]), Verdict::Found);
        assert_eq!(fetch_bibtex(&servers, "10.1038/pearl-does-not-exist-123"), Err("the-doi-was-not-found"));
        let by_title = crate::bib::parse_bib("@article{a, title = {Attention Is All You Need}, journal = {J}, year = {2017}}").unwrap();
        let verdict = check(&servers, &by_title[0]);
        println!("by title: {verdict:?}");
        assert_ne!(verdict, Verdict::Failed);
        let invented = crate::bib::parse_bib(
            "@book{b, title = {Tratado imaginario sobre perolas azuis de Marte em dialeto antigo}, publisher = {P}, year = {2001}}",
        )
        .unwrap();
        assert_eq!(check(&servers, &invented[0]), Verdict::NotFound);
    }

    #[test]
    fn a_doi_is_found_in_what_is_pasted() {
        for input in [
            "10.1038/nature14539",
            " doi:10.1038/nature14539 ",
            "https://doi.org/10.1038/nature14539",
            "http://dx.doi.org/10.1038/nature14539.",
            "(10.1038/nature14539)",
        ] {
            assert_eq!(extract(input).as_deref(), Some("10.1038/nature14539"), "{input}");
        }
        assert_eq!(extract("10.1000/a(b)c/1").as_deref(), Some("10.1000/a(b)c/1"));
        for input in ["", "nature14539", "10.1038", "10.1038/", "10.12/abc", "10.abcd/efg", "versão 10.5"] {
            assert_eq!(extract(input), None, "{input}");
        }
    }

    #[test]
    fn what_goes_in_an_address_is_encoded() {
        assert_eq!(encode("10.1000/a b?c#d", true), "10.1000/a%20b%3Fc%23d");
        assert_eq!(encode("Pérola & co/x", false), "P%C3%A9rola%20%26%20co%2Fx");
    }

    #[test]
    fn titles_are_compared_for_the_work_not_the_writing() {
        for (a, b) in [
            ("Deep learning", "Deep Learning"),
            ("Sobre as pérolas", "SOBRE AS PEROLAS"),
            ("Deep learning & networks", "Deep learning &amp; networks"),
            ("The TeXbook", "The TeXbook: a guide"),
        ] {
            assert!(same_title(a, b), "{a} / {b}");
        }
        for (a, b) in [
            ("Deep learning", "Deep learning for the analysis of ocean pearls in tropical waters"),
            ("Deep learning", "Shallow learning"),
            ("Pearls", "Pearl"),
            ("", ""),
            ("Attention is all you need", "Attention is what you want"),
        ] {
            assert!(!same_title(a, b), "{a} / {b}");
        }
    }

    #[test]
    fn replies_of_any_shape_are_survived() {
        assert_eq!(title_in_csl(r#"{"title":"T"}"#).as_deref(), Some("T"));
        assert_eq!(title_in_csl(r#"{"title":["T"]}"#).as_deref(), Some("T"));
        for body in ["", "<html>", "[]", "{}", r#"{"title":3}"#, r#"{"title":[]}"#] {
            assert_eq!(title_in_csl(body), None, "{body}");
        }
        assert_eq!(titles_in_crossref(r#"{"message":{"items":[{"title":["A"]},{"title":[]},{}]}}"#), Some(vec!["A".to_string()]));
        for body in ["", "{}", r#"{"message":{}}"#, r#"{"message":{"items":3}}"#] {
            assert_eq!(titles_in_crossref(body), None, "{body}");
        }
    }
}
