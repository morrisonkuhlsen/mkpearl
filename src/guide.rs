//! The documentation in the side pane: lessons written for Pearl, in `locales/guide/`, and a reference
//! of every function, from the documentation Typst carries inside itself, which is in English.

use std::sync::OnceLock;

use typst::LibraryExt;
use typst::foundations::{Func, Scope, Value};

use crate::bib::fold;
use crate::i18n::{Lang, current};

/// One piece of a page of documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(String),
    Text(String),
    /// Code to read only: it needs files, or is a fragment.
    Code(String),
    /// Code that runs, and can be edited, in the pane.
    Example(String),
    /// A suggestion of something to change in the example above.
    Try(String),
}

#[derive(Debug)]
pub struct Lesson {
    pub category: String,
    pub title: String,
    pub blocks: Vec<Block>,
    haystack: String,
}

#[derive(Debug)]
pub struct Function {
    /// As it's called, with its module: `calc.round`.
    pub name: String,
    pub title: String,
    pub blocks: Vec<Block>,
    haystack: String,
}

/// What a search finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Lesson(usize),
    Function(usize),
}

const MAX_HITS: usize = 60;
/// Modules nested inside modules, deep enough for what Typst has.
const MAX_DEPTH: usize = 4;

/// The lessons of a guide's text.
fn parse_lessons(text: &str) -> Vec<Lesson> {
    let mut lessons: Vec<Lesson> = Vec::new();
    let mut category = String::new();
    let mut lines = text.lines().peekable();
    let mut body = String::new();
    let mut title = None;
    let mut keywords = String::new();
    let mut finish = |title: Option<String>, category: &str, keywords: &str, body: &str| {
        if let Some(title) = title {
            let haystack = fold(&format!("{title} {keywords} {body}"));
            lessons.push(Lesson { category: category.to_string(), title, blocks: parse_blocks(body), haystack });
        }
    };
    while let Some(line) = lines.next() {
        if let Some(name) = line.strip_prefix("# ") {
            finish(title.take(), &category, &keywords, &body);
            body.clear();
            category = name.to_string();
        } else if let Some(name) = line.strip_prefix("## ") {
            finish(title.take(), &category, &keywords, &body);
            body.clear();
            title = Some(name.to_string());
            keywords = lines.next_if(|l| l.starts_with("?? ")).map_or_else(String::new, |l| l.trim_start_matches("?? ").to_string());
        } else {
            body += line;
            body.push('\n');
        }
    }
    finish(title.take(), &category, &keywords, &body);
    lessons
}

/// Blocks from Markdown: the guide's, or Typst's, whose ` ```example ` blocks run. In those, `>>>` marks
/// a line that runs but isn't shown, and `<<<` one that's shown but doesn't run.
pub fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut fence: Option<(bool, String)> = None;
    let flush = |paragraph: &mut String, blocks: &mut Vec<Block>| {
        let text = paragraph.trim();
        if !text.is_empty() {
            blocks.push(Block::Text(inline_links(text)));
        }
        paragraph.clear();
    };
    for line in text.lines() {
        if let Some((runs, code)) = &mut fence {
            if line.trim_start().starts_with("```") {
                let code = code.trim_end().to_string();
                blocks.push(if *runs { Block::Example(code) } else { Block::Code(code) });
                fence = None;
            } else if *runs {
                let kept = line.strip_prefix(">>>").map(|l| l.strip_prefix(' ').unwrap_or(l)).or_else(|| (!line.starts_with("<<<")).then_some(line));
                if let Some(kept) = kept {
                    code.push_str(kept);
                    code.push('\n');
                }
            } else {
                code.push_str(line);
                code.push('\n');
            }
        } else if let Some(info) = line.trim_start().strip_prefix("```") {
            flush(&mut paragraph, &mut blocks);
            fence = Some((info.trim_start().starts_with("example"), String::new()));
        } else if let Some(heading) = line.strip_prefix('#').map(|l| l.trim_start_matches('#')).and_then(|l| l.strip_prefix(' ')) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Heading(inline_links(heading.trim())));
        } else if let Some(suggestion) = line.strip_prefix("> ") {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Try(inline_links(suggestion)));
        } else if line.trim().is_empty() {
            flush(&mut paragraph, &mut blocks);
        } else {
            if !paragraph.is_empty() {
                paragraph.push(if line.trim_start().starts_with(['-', '+']) { '\n' } else { ' ' });
            }
            paragraph.push_str(line.trim());
        }
    }
    flush(&mut paragraph, &mut blocks);
    if let Some((_, code)) = fence {
        blocks.push(Block::Code(code));
    }
    blocks
}

/// `[words](address)` as just the words, since the pane shows no addresses, and `**` taken out.
fn inline_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest.get(open..).and_then(|r| r.find("](")).map(|c| c + open) else { break };
        let Some(end) = rest.get(close + 2..).and_then(|r| r.find(')')).map(|e| e + close + 2) else { break };
        let (Some(before), Some(words)) = (rest.get(..open), rest.get(open + 1..close)) else { break };
        out += before;
        out += words;
        let Some(after) = rest.get(end + 1..) else { break };
        rest = after;
    }
    out += rest;
    out.replace("**", "")
}

/// Splits text into plain and `code` parts, in order.
pub fn inline_parts(text: &str) -> Vec<(&str, bool)> {
    text.split('`').enumerate().filter(|(_, part)| !part.is_empty()).map(|(i, part)| (part, i % 2 == 1)).collect()
}

pub fn lessons() -> &'static [Lesson] {
    static PT: OnceLock<Vec<Lesson>> = OnceLock::new();
    static EN: OnceLock<Vec<Lesson>> = OnceLock::new();
    match current() {
        Lang::PtBr => PT.get_or_init(|| parse_lessons(include_str!("../locales/guide/pt-BR.md"))),
        Lang::En => EN.get_or_init(|| parse_lessons(include_str!("../locales/guide/en.md"))),
    }
}

pub fn functions() -> &'static [Function] {
    static ALL: OnceLock<Vec<Function>> = OnceLock::new();
    ALL.get_or_init(|| {
        let library = typst::Library::default();
        let mut found = Vec::new();
        walk(library.global.scope(), "", 0, &mut found);
        found.sort_by(|a: &Function, b| a.name.cmp(&b.name));
        found.dedup_by(|a, b| a.name == b.name);
        found
    })
}

fn walk(scope: &Scope, prefix: &str, depth: usize, found: &mut Vec<Function>) {
    if depth > MAX_DEPTH {
        return;
    }
    for (name, binding) in scope.iter() {
        let path = format!("{prefix}{name}");
        match binding.read() {
            Value::Func(func) => {
                add_function(func, &path, found);
                if let Some(inner) = func.scope() {
                    walk(inner, &format!("{path}."), depth + 1, found);
                }
            }
            Value::Type(ty) => {
                if let Ok(constructor) = ty.constructor() {
                    add_function(&constructor, &path, found);
                }
                walk(ty.scope(), &format!("{path}."), depth + 1, found);
            }
            Value::Module(module) => walk(module.scope(), &format!("{path}."), depth + 1, found),
            _ => {}
        }
    }
}

fn add_function(func: &Func, path: &str, found: &mut Vec<Function>) {
    let Some(docs) = func.docs() else { return };
    let mut text = docs.to_string();
    let params: Vec<String> = func
        .params()
        .filter_map(|param| param.to_native().map(|info| format!("- `{}`: {}", info.name, info.docs.lines().next().unwrap_or_default())))
        .collect();
    let keywords = func.keywords().join(" ");
    let mut blocks = parse_blocks(&text);
    if !params.is_empty() {
        blocks.push(Block::Heading("Parameters".to_string()));
        blocks.push(Block::Text(inline_links(&params.join("\n"))));
        text.push_str(&params.join(" "));
    }
    let title = func.title().unwrap_or(path).to_string();
    let haystack = fold(&format!("{path} {title} {keywords} {text}"));
    found.push(Function { name: path.to_string(), title, blocks, haystack });
}

/// Lessons and functions with every word of the search, what's named so first.
pub fn search(query: &str) -> Vec<Hit> {
    let folded = fold(query);
    let words: Vec<&str> = folded.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let has_all = |haystack: &str| words.iter().all(|word| haystack.contains(word));
    let rank = |name: &str| {
        let name = fold(name);
        if name == folded {
            0
        } else if name.starts_with(&folded) {
            1
        } else if name.contains(&folded) {
            2
        } else {
            3
        }
    };
    let mut hits: Vec<(u8, Hit)> = Vec::new();
    for (i, lesson) in lessons().iter().enumerate() {
        if has_all(&lesson.haystack) {
            hits.push((rank(&lesson.title), Hit::Lesson(i)));
        }
    }
    for (i, function) in functions().iter().enumerate() {
        if has_all(&function.haystack) {
            // At the same rank a lesson teaches first.
            hits.push((rank(&function.name).max(1), Hit::Function(i)));
        }
    }
    hits.sort_by_key(|(rank, _)| *rank);
    hits.truncate(MAX_HITS);
    hits.into_iter().map(|(_, hit)| hit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{self, Lang};

    fn both() -> [Vec<Lesson>; 2] {
        [parse_lessons(include_str!("../locales/guide/pt-BR.md")), parse_lessons(include_str!("../locales/guide/en.md"))]
    }

    #[test]
    fn both_languages_have_the_same_lessons_in_the_same_shape() {
        let [pt, en] = both();
        assert!(pt.len() >= 20);
        assert_eq!(pt.len(), en.len());
        for (a, b) in pt.iter().zip(&en) {
            let shape = |lesson: &Lesson| lesson.blocks.iter().map(std::mem::discriminant).collect::<Vec<_>>();
            assert_eq!(shape(a), shape(b), "{} / {}", a.title, b.title);
        }
    }

    #[test]
    fn markdown_becomes_blocks() {
        let blocks = parse_blocks(
            "Texto\ncontinua [aqui]($x) e **ali**.\n\n# Título\n```example\n>>> oculto\nvisto\n<<< só mostrado\n```\n> Tente\n```typ\ncode\n```\n- a\n- b\n",
        );
        assert_eq!(
            blocks,
            [
                Block::Text("Texto continua aqui e ali.".to_string()),
                Block::Heading("Título".to_string()),
                Block::Example("oculto\nvisto".to_string()),
                Block::Try("Tente".to_string()),
                Block::Code("code".to_string()),
                Block::Text("- a\n- b".to_string()),
            ]
        );
        assert_eq!(inline_parts("usa `x` e `y`"), [("usa ", false), ("x", true), (" e ", false), ("y", true)]);
        for broken in ["[", "](", "[a](", "a]b(c)", "```", "```example\nx"] {
            let _ = parse_blocks(broken);
        }
    }

    #[test]
    fn the_reference_covers_typst() {
        let all = functions();
        assert!(all.len() > 200, "{}", all.len());
        for name in ["text", "image", "table", "calc.round", "bibliography", "page", "figure"] {
            assert!(all.iter().any(|f| f.name == name), "{name}");
        }
        let table = all.iter().find(|f| f.name == "table").unwrap();
        assert!(table.blocks.iter().any(|b| matches!(b, Block::Example(_))));
        assert!(table.blocks.iter().any(|b| matches!(b, Block::Text(t) if t.contains("`columns`"))));
    }

    #[test]
    fn a_search_finds_lessons_and_functions_without_accents() {
        i18n::set(Lang::PtBr);
        let lesson = |hit: &Hit| match hit {
            Hit::Lesson(i) => lessons()[*i].title.clone(),
            Hit::Function(i) => functions()[*i].name.clone(),
        };
        let found: Vec<String> = search("matematica").iter().map(lesson).collect();
        assert!(found.iter().any(|t| t == "Fórmulas"), "{found:?}");
        let tables: Vec<String> = search("tabela").iter().map(lesson).collect();
        assert_eq!(tables.first().map(String::as_str), Some("Tabelas"), "{tables:?}");
        let exact: Vec<String> = search("table").iter().map(lesson).collect();
        assert!(exact.iter().take(3).any(|t| t == "table"), "{exact:?}");
        assert!(search("   ").is_empty());
        assert!(search("xyzzyqqq").is_empty());
        assert!(search("a").len() <= MAX_HITS);
    }
}
