//! Pearl in the user's language. One message catalog per language, in a subset of Fluent (`.ftl`):
//! `id = text`, continuation lines indented, `{ $name }` placeholders, and plurals as
//! `{ $n -> [one] … *[other] … }`. The catalogs are embedded; a message missing from one falls back
//! to English, and one missing from English shows its id, so nothing ever fails to show. Adapted
//! from MK Perth's.
//!
//! The language is the program's, as the workers' messages reach the window: `set` it, and `t` and
//! `tr!` read it. Tests run in Portuguese unless they set another. What goes into a document is
//! not translated here.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// A language the interface speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Lang {
    En,
    #[default]
    PtBr,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::PtBr];

    /// Its BCP 47 tag, as saved in the settings.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::PtBr => "pt-BR",
        }
    }

    /// Its name in itself, for the menu.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::PtBr => "Português (Brasil)",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    /// From a POSIX locale such as `pt_BR.UTF-8` (the `LANG` variable): Portuguese for any `pt`,
    /// English otherwise.
    pub fn from_locale(locale: &str) -> Lang {
        let lower = locale.to_ascii_lowercase();
        if lower.starts_with("pt") { Lang::PtBr } else { Lang::En }
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../locales/en.ftl"),
            Lang::PtBr => include_str!("../locales/pt-BR.ftl"),
        }
    }

    /// CLDR's cardinal plural category of `n`: `one` or `other` (all these languages need).
    fn plural(self, n: f64) -> &'static str {
        let one = match self {
            Lang::En => n == 1.0,
            // Brazilian Portuguese: 0 and 1 take the singular.
            Lang::PtBr => n == 0.0 || n == 1.0,
        };
        if one { "one" } else { "other" }
    }

    fn catalog(self) -> &'static Catalog {
        static CATALOGS: [OnceLock<Catalog>; 2] = [OnceLock::new(), OnceLock::new()];
        let index = match self {
            Lang::En => 0,
            Lang::PtBr => 1,
        };
        CATALOGS.get(index).map_or_else(empty, |c| c.get_or_init(|| parse(self.source()).0))
    }
}

fn empty() -> &'static Catalog {
    static EMPTY: OnceLock<Catalog> = OnceLock::new();
    EMPTY.get_or_init(HashMap::new)
}

const LANGUAGE_FILE: &str = "lang.txt";
static CURRENT: AtomicU8 = AtomicU8::new(1);

/// Sets the program's language.
pub fn set(lang: Lang) {
    CURRENT.store(u8::from(lang == Lang::PtBr), Ordering::Relaxed);
}

/// The program's language.
pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 { Lang::PtBr } else { Lang::En }
}

/// The language saved in the settings, or else the system's.
pub fn load() -> Lang {
    let saved = crate::recent::read_setting(LANGUAGE_FILE).and_then(|code| Lang::from_code(&code));
    saved.unwrap_or_else(|| Lang::from_locale(&std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")).unwrap_or_default()))
}

/// Sets the language and keeps it for next time.
pub fn choose(lang: Lang) {
    set(lang);
    crate::recent::write_setting(LANGUAGE_FILE, lang.code());
}

/// A message without placeholders, for the many places that want a plain `&str`.
pub fn tx(id: &'static str) -> &'static str {
    let lang = current();
    match lang.catalog().get(id).or_else(|| Lang::En.catalog().get(id)).map(Vec::as_slice) {
        Some([Piece::Text(text)]) => text,
        _ => id,
    }
}

/// A value for a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Str(String),
    Num(f64),
}

impl From<&str> for Arg {
    fn from(s: &str) -> Self {
        Arg::Str(s.to_string())
    }
}
impl From<String> for Arg {
    fn from(s: String) -> Self {
        Arg::Str(s)
    }
}
impl From<&String> for Arg {
    fn from(s: &String) -> Self {
        Arg::Str(s.clone())
    }
}
impl From<f64> for Arg {
    fn from(x: f64) -> Self {
        Arg::Num(x)
    }
}
impl From<usize> for Arg {
    fn from(x: usize) -> Self {
        Arg::Num(x as f64)
    }
}
impl From<i64> for Arg {
    fn from(x: i64) -> Self {
        Arg::Num(x as f64)
    }
}
macro_rules! from_integer {
    ($($t:ty),*) => {$(
        impl From<$t> for Arg {
            fn from(x: $t) -> Self {
                Arg::Num(f64::from(x))
            }
        }
    )*};
}
from_integer!(i32, u32, u16, u8);
impl From<u64> for Arg {
    fn from(x: u64) -> Self {
        Arg::Num(x as f64)
    }
}

/// A piece of a message.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Var(String),
    /// `{ $var -> [key] … *[default] … }`: the variants by key, and the default's index.
    Select {
        var: String,
        variants: Vec<(String, Vec<Piece>)>,
        default: usize,
    },
}

type Catalog = HashMap<String, Vec<Piece>>;

/// The message `id` in this thread's language with its placeholders filled.
pub fn message(id: &str, args: &[(&str, Arg)]) -> String {
    message_in(current(), id, args)
}

/// The message `id` in `lang`.
pub fn message_in(lang: Lang, id: &str, args: &[(&str, Arg)]) -> String {
    let pieces = lang.catalog().get(id).or_else(|| Lang::En.catalog().get(id));
    match pieces {
        Some(pieces) => {
            let mut out = String::new();
            render(lang, pieces, args, &mut out);
            out
        }
        None => id.to_string(),
    }
}

/// A number as a placeholder shows it: whole numbers without decimals.
fn show(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

fn render(lang: Lang, pieces: &[Piece], args: &[(&str, Arg)], out: &mut String) {
    let arg = |name: &str| args.iter().find(|(n, _)| *n == name).map(|(_, a)| a);
    for piece in pieces {
        match piece {
            Piece::Text(s) => out.push_str(s),
            Piece::Var(name) => match arg(name) {
                Some(Arg::Str(s)) => out.push_str(s),
                Some(Arg::Num(x)) => out.push_str(&show(*x)),
                None => {
                    out.push_str("{$");
                    out.push_str(name);
                    out.push('}');
                }
            },
            Piece::Select { var, variants, default } => {
                let key = match arg(var) {
                    Some(Arg::Num(x)) => Some(lang.plural(*x).to_string()),
                    Some(Arg::Str(s)) => Some(s.clone()),
                    None => None,
                };
                // An exact number first ([0]), then the plural category, then the default.
                let exact = match arg(var) {
                    Some(Arg::Num(x)) => Some(show(*x)),
                    _ => None,
                };
                let chosen = variants
                    .iter()
                    .find(|(k, _)| Some(k) == exact.as_ref())
                    .or_else(|| variants.iter().find(|(k, _)| Some(k) == key.as_ref()))
                    .or_else(|| variants.get(*default));
                if let Some((_, pieces)) = chosen {
                    render(lang, pieces, args, out);
                }
            }
        }
    }
}

/// Parses a catalog; the second part lists what couldn't be read (for the tests).
fn parse(source: &str) -> (Catalog, Vec<String>) {
    let mut catalog = Catalog::new();
    let mut errors = Vec::new();
    // Messages: an id at the start of a line, its value continued on indented lines.
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Indented lines continue the message, and so does the `}` that closes a select.
        if line.starts_with(' ') || line.starts_with('\t') || trimmed.starts_with('}') {
            match entries.last_mut() {
                Some((_, value)) => {
                    if !value.is_empty() {
                        value.push(if trimmed.starts_with('[') || trimmed.starts_with("*[") || trimmed.starts_with('}') { ' ' } else { '\n' });
                    }
                    value.push_str(trimmed);
                }
                None => errors.push(format!("a continuation without a message: {line}")),
            }
            continue;
        }
        match line.split_once('=') {
            Some((id, value)) if valid_id(id.trim()) => entries.push((id.trim().to_string(), value.trim().to_string())),
            _ => errors.push(format!("not a message: {line}")),
        }
    }
    for (id, value) in entries {
        match pattern(&value) {
            Ok(pieces) => {
                if catalog.insert(id.clone(), pieces).is_some() {
                    errors.push(format!("{id} twice"));
                }
            }
            Err(e) => errors.push(format!("{id}: {e}")),
        }
    }
    (catalog, errors)
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A message's value: text, `{ $var }` and `{ $var -> … }`.
fn pattern(value: &str) -> Result<Vec<Piece>, String> {
    let chars: Vec<char> = value.chars().collect();
    let (pieces, end) = pieces(&chars, 0, false)?;
    if end < chars.len() { Err(format!("unexpected '{}'", chars.get(end).copied().unwrap_or(' '))) } else { Ok(pieces) }
}

/// Pieces from `at` until the end, or (in a variant) until the next `[`, `*[` or `}`.
fn pieces(chars: &[char], mut at: usize, in_variant: bool) -> Result<(Vec<Piece>, usize), String> {
    let mut out = Vec::new();
    let mut text = String::new();
    while let Some(&c) = chars.get(at) {
        if in_variant && (c == '}' || c == '[' || (c == '*' && chars.get(at + 1) == Some(&'['))) {
            break;
        }
        if c == '{' {
            if !text.is_empty() {
                out.push(Piece::Text(std::mem::take(&mut text)));
            }
            let (piece, next) = placeable(chars, at + 1)?;
            out.push(piece);
            at = next;
            continue;
        }
        text.push(c);
        at += 1;
    }
    if in_variant {
        let trimmed = text.trim_end().to_string();
        if !trimmed.is_empty() {
            out.push(Piece::Text(trimmed));
        }
    } else if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    Ok((out, at))
}

fn skip_space(chars: &[char], mut at: usize) -> usize {
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    at
}

/// After a `{`: a `$var`, a select on it, or a quoted literal (`{ "{" }`); returns after `}`.
fn placeable(chars: &[char], at: usize) -> Result<(Piece, usize), String> {
    let mut at = skip_space(chars, at);
    if chars.get(at) == Some(&'"') {
        let mut literal = String::new();
        at += 1;
        while let Some(&c) = chars.get(at) {
            at += 1;
            if c == '"' {
                break;
            }
            literal.push(c);
        }
        at = skip_space(chars, at);
        return if chars.get(at) == Some(&'}') { Ok((Piece::Text(literal), at + 1)) } else { Err("an unclosed literal".into()) };
    }
    if chars.get(at) != Some(&'$') {
        return Err("a placeable that isn't a $variable".into());
    }
    at += 1;
    let mut name = String::new();
    while let Some(&c) = chars.get(at).filter(|c| c.is_ascii_alphanumeric() || **c == '-' || **c == '_') {
        name.push(c);
        at += 1;
    }
    at = skip_space(chars, at);
    match chars.get(at) {
        Some('}') => Ok((Piece::Var(name), at + 1)),
        Some('-') if chars.get(at + 1) == Some(&'>') => {
            at += 2;
            let mut variants = Vec::new();
            let mut default = None;
            loop {
                at = skip_space(chars, at);
                match chars.get(at) {
                    Some('}') => break,
                    Some('*') | Some('[') => {
                        if chars.get(at) == Some(&'*') {
                            default = Some(variants.len());
                            at += 1;
                        }
                        if chars.get(at) != Some(&'[') {
                            return Err("a variant without its [key]".into());
                        }
                        let mut key = String::new();
                        at += 1;
                        while let Some(&c) = chars.get(at) {
                            at += 1;
                            if c == ']' {
                                break;
                            }
                            key.push(c);
                        }
                        let start = skip_space(chars, at);
                        let (body, next) = pieces(chars, start, true)?;
                        variants.push((key.trim().to_string(), body));
                        at = next;
                    }
                    _ => return Err("an unclosed select".into()),
                }
            }
            let default = default.ok_or("a select without a *[default]")?;
            Ok((Piece::Select { var: name, variants, default }, at + 1))
        }
        _ => Err(format!("an unclosed {{ ${name}")),
    }
}

/// `tr!("id")` or `tr!("id", name = value, …)`: the message in this thread's language.
#[macro_export]
macro_rules! tr {
    ($id:expr) => {
        $crate::i18n::message($id, &[])
    };
    ($id:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::message($id, &[$((stringify!($name), $crate::i18n::Arg::from($value))),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// `tx` is for messages without placeholders; what has them goes through `tr!`.
    #[test]
    fn a_plain_message_is_plain_text_and_an_unknown_one_shows_its_id() {
        assert_eq!(tx("no-such-message"), "no-such-message");
        assert_eq!(message_in(Lang::En, "pages-count", &[("n", Arg::from(1usize))]), "1 page");
        assert_eq!(message_in(Lang::En, "pages-count", &[("n", Arg::from(3usize))]), "3 pages");
        assert_eq!(message_in(Lang::PtBr, "pages-count", &[("n", Arg::from(3usize))]), "3 páginas");
    }

    fn variables(pieces: &[Piece], out: &mut BTreeSet<String>) {
        for p in pieces {
            match p {
                Piece::Text(_) => {}
                Piece::Var(v) => {
                    out.insert(v.clone());
                }
                Piece::Select { var, variants, .. } => {
                    out.insert(var.clone());
                    for (_, body) in variants {
                        variables(body, out);
                    }
                }
            }
        }
    }

    #[test]
    fn every_catalog_parses_and_has_every_message_with_the_same_placeholders() {
        let (en, errors) = parse(Lang::En.source());
        assert!(errors.is_empty(), "en: {errors:?}");
        for lang in Lang::ALL {
            let (catalog, errors) = parse(lang.source());
            assert!(errors.is_empty(), "{}: {errors:?}", lang.code());
            let missing: Vec<&String> = en.keys().filter(|k| !catalog.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{} lacks {missing:?}", lang.code());
            let extra: Vec<&String> = catalog.keys().filter(|k| !en.contains_key(*k)).collect();
            assert!(extra.is_empty(), "{} has messages English lacks: {extra:?}", lang.code());
            for (id, pieces) in &catalog {
                let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
                variables(pieces, &mut a);
                variables(en.get(id).map_or(&[][..], Vec::as_slice), &mut b);
                assert_eq!(a, b, "{}: {id}'s placeholders", lang.code());
            }
        }
    }

    /// Every `tr!("id"` and `tx("id"` in the sources names a message English has.
    #[test]
    fn every_message_the_code_asks_for_exists() {
        let (en, _) = parse(Lang::En.source());
        let mut missing = Vec::new();
        let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap_or_default();
                    for mark in ["tr!(\"", "tx(\""] {
                        let chunks: Vec<&str> = source.split(mark).collect();
                        for (before, chunk) in chunks.iter().zip(chunks.iter().skip(1)) {
                            // Itself, not the end of another name (`include_str!`).
                            if before.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                                continue;
                            }
                            let id: String = chunk.chars().take_while(|c| *c != '"').collect();
                            // This file's own examples.
                            if !["id", "{", "no-such-message"].contains(&id.as_str()) && !en.contains_key(&id) {
                                missing.push(format!("{}: {id}", path.display()));
                            }
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn placeholders_plurals_and_fallbacks() {
        let (catalog, errors) = parse(
            "greet = Hello, { $name }!\ncases = { $n ->\n    [one] one case\n   *[other] { $n } cases\n}\nbrace = a { \"{\" } b\nlong = first\n    second",
        );
        assert!(errors.is_empty(), "{errors:?}");
        let say = |id: &str, args: &[(&str, Arg)], lang: Lang| {
            let mut out = String::new();
            render(lang, catalog.get(id).map_or(&[][..], Vec::as_slice), args, &mut out);
            out
        };
        assert_eq!(say("greet", &[("name", "Ana".into())], Lang::En), "Hello, Ana!");
        assert_eq!(say("cases", &[("n", 1usize.into())], Lang::En), "one case");
        assert_eq!(say("cases", &[("n", 3usize.into())], Lang::En), "3 cases");
        assert_eq!(say("cases", &[("n", 0usize.into())], Lang::En), "0 cases");
        assert_eq!(say("cases", &[("n", 0usize.into())], Lang::PtBr), "one case", "0 is singular in Brazilian Portuguese");
        assert_eq!(say("brace", &[], Lang::En), "a { b");
        assert_eq!(say("long", &[], Lang::En), "first\nsecond");
        // Unknown ids show themselves.
        assert_eq!(message_in(Lang::PtBr, "no-such-message", &[]), "no-such-message");
        assert_eq!(Lang::from_locale("pt_BR.UTF-8"), Lang::PtBr);
        assert_eq!(Lang::from_locale("C"), Lang::En);
        assert_eq!(Lang::from_code("PT-br"), Some(Lang::PtBr));
    }
}
