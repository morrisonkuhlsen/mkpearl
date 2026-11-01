//! Typst syntax colours for the editor, from Typst's own parser.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Stroke};
use typst::syntax::{LinkedNode, SyntaxKind, Tag, highlight, parse};

use crate::theme::{Syntax, Tokens};

fn color(tag: Tag, s: &Style) -> Color32 {
    let code = s.syntax;
    match tag {
        Tag::Comment => code.comment,
        Tag::Heading | Tag::ListTerm | Tag::Keyword | Tag::Operator | Tag::MathOperator | Tag::MathDelimiter => code.keyword,
        Tag::ListMarker | Tag::Escape | Tag::Function | Tag::Interpolated => code.builtin,
        Tag::String | Tag::Raw | Tag::Link => code.string,
        Tag::Number | Tag::Label | Tag::Ref => code.number,
        Tag::Error => s.danger,
        _ => s.base,
    }
}

struct Style<'a> {
    font: FontId,
    base: Color32,
    danger: Color32,
    syntax: Syntax,
    /// Whether a word is known to be misspelt.
    misspelt: Option<&'a dyn Fn(&str) -> bool>,
}

/// Text with its misspelt words underlined.
fn append_text(job: &mut LayoutJob, text: &str, format: &TextFormat, style: &Style) {
    let Some(misspelt) = style.misspelt else {
        job.append(text, 0.0, format.clone());
        return;
    };
    let mut at = 0;
    for word in crate::spell::word_ranges(text) {
        let Some(this) = text.get(word.clone()).filter(|w| misspelt(w)) else { continue };
        job.append(text.get(at..word.start).unwrap_or_default(), 0.0, format.clone());
        let mut marked = format.clone();
        marked.underline = Stroke::new(1.5, style.danger);
        job.append(this, 0.0, marked);
        at = word.end;
    }
    job.append(text.get(at..).unwrap_or_default(), 0.0, format.clone());
}

fn walk(job: &mut LayoutJob, src: &str, node: &LinkedNode, inherited: Option<Tag>, style: &Style) {
    let tag = highlight(node).or(inherited);
    if node.children().len() == 0 {
        let mut format = TextFormat::simple(style.font.clone(), tag.map_or(style.base, |t| color(t, style)));
        format.italics = matches!(tag, Some(Tag::Emph | Tag::Comment));
        let leaf = src.get(node.range()).unwrap_or_default();
        if node.kind() == SyntaxKind::Text {
            append_text(job, leaf, &format, style);
        } else {
            job.append(leaf, 0.0, format);
        }
        return;
    }
    for child in node.children() {
        walk(job, src, &child, tag, style);
    }
}

/// The source as coloured text. Parsing is error tolerant and keeps every byte of the source.
pub fn job(text: &str, font: FontId, t: &Tokens, misspelt: Option<&dyn Fn(&str) -> bool>) -> LayoutJob {
    let root = parse(text);
    let mut job = LayoutJob::default();
    walk(&mut job, text, &LinkedNode::new(&root), None, &Style { font, base: t.text, danger: t.danger, syntax: t.syntax, misspelt });
    job
}

/// One heading of the document: its level, its text and where it starts.
#[derive(Debug, PartialEq)]
pub struct Heading {
    pub level: usize,
    pub title: String,
    pub byte: usize,
}

fn collect_headings(src: &str, node: &LinkedNode, out: &mut Vec<Heading>) {
    if node.kind() == SyntaxKind::Heading {
        let text = src.get(node.range()).unwrap_or_default();
        let level = text.chars().take_while(|c| *c == '=').count();
        out.push(Heading { level, title: text.trim_start_matches('=').trim().to_string(), byte: node.offset() });
        return;
    }
    for child in node.children() {
        collect_headings(src, &child, out);
    }
}

/// The document's headings, from Typst's parser, so a `=` in code or a raw block isn't one.
pub fn outline(text: &str) -> Vec<Heading> {
    let root = parse(text);
    let mut out = Vec::new();
    collect_headings(text, &LinkedNode::new(&root), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outline_finds_real_headings_only() {
        let found = outline("= Início\ntexto\n```\n= não\n```\n#let a = 1\n== Fim de *tudo*\n");
        assert_eq!(found.iter().map(|h| (h.level, h.title.as_str())).collect::<Vec<_>>(), [(1, "Início"), (2, "Fim de *tudo*")]);
        assert_eq!(found.first().map(|h| h.byte), Some(0));
    }

    #[test]
    fn highlighting_keeps_the_text_intact() {
        for src in ["= Título\n*forte* $x^2$ #let a = 1\n", "#table(", "$ unclosed", "", "/* c"] {
            assert_eq!(job(src, FontId::monospace(12.0), &Tokens::dark(), None).text, src);
        }
    }
}
