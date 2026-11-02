//! Letterhead and watermark: settings, the `#set page` they become, and the block that holds them at
//! the top of a document. The settings go along in a comment, so the dialog can open on them again.
//! That comment is part of a document, and so hostile input: it's read within limits, with defaults.

use std::ops::Range;

use serde_json::{Value, json};

use crate::edit::{self, Sel};

const START: &str = "// pearl:letterhead ";
const END: &str = "// pearl:end";
pub const SHADES: std::ops::RangeInclusive<f32> = 150.0..=245.0;
pub const SIZES: std::ops::RangeInclusive<f32> = 20.0..=140.0;
pub const ANGLES: std::ops::RangeInclusive<f32> = -90.0..=90.0;
pub const OPACITIES: std::ops::RangeInclusive<f32> = 5.0..=100.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Letterhead {
    pub head_on: bool,
    pub name: String,
    pub details: String,
    pub footer: String,
    pub logo: String,
    pub first_page_only: bool,
    pub mark_on: bool,
    pub mark_is_image: bool,
    pub mark_text: String,
    pub mark_image: String,
    pub mark_angle: f32,
    pub mark_size: f32,
    pub mark_shade: f32,
    /// How much of a picture shows through, in percent.
    pub mark_opacity: f32,
}

impl Default for Letterhead {
    fn default() -> Self {
        Self {
            head_on: false,
            name: String::new(),
            details: String::new(),
            footer: String::new(),
            logo: String::new(),
            first_page_only: false,
            mark_on: false,
            mark_is_image: false,
            mark_text: String::new(),
            mark_image: String::new(),
            mark_angle: -45.0,
            mark_size: 70.0,
            mark_shade: 225.0,
            mark_opacity: 25.0,
        }
    }
}

/// Text as a Typst string, so that nothing in it is read as markup or code.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' | '\t' => out.push(' '),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

impl Letterhead {
    fn picture(path: &str, placeholder: &str, real: &str, preview: bool) -> String {
        if preview { placeholder.to_string() } else { format!("image({}, {real})", quoted(path)) }
    }

    /// The picture of a watermark at its opacity. Typst has no opacity for images, so a veil of the
    /// paper's own colour goes over it, which comes to the same on a plain page and works for every format.
    fn faded_mark(&self, preview: bool) -> String {
        let opacity = self.mark_opacity.round();
        if preview {
            return format!("rect(width: 60%, height: 30%, fill: luma(120).transparentize({}%))", 100.0 - opacity);
        }
        let path = quoted(&self.mark_image);
        if opacity >= *OPACITIES.end() {
            return format!("image({path}, width: 70%)");
        }
        format!(
            "layout(size => {{ let pic = image({path}, width: 0.7 * size.width); let paper = if type(page.fill) == color {{ page.fill }} else {{ white }}; \
             let drawn = measure(pic); box(width: drawn.width, height: drawn.height, {{ place(pic); \
             place(rect(width: 100%, height: 100%, fill: paper.transparentize({opacity}%))) }}) }})"
        )
    }

    /// The rule, or nothing when nothing's on. With `preview`, grey boxes stand in for pictures, which
    /// a preview can't read.
    pub fn rule(&self, preview: bool) -> Option<String> {
        let mut parts = Vec::new();
        let once =
            |content: String| if self.first_page_only { format!("context if counter(page).get().first() == 1 {{ {content} }}") } else { content };
        if self.head_on {
            let mut cells = Vec::new();
            if !self.logo.is_empty() {
                cells.push(Self::picture(&self.logo, "rect(width: 1cm, height: 1cm, fill: luma(210))", "height: 1cm", preview));
            }
            cells.push(format!("text(13pt, weight: \"bold\", {})", quoted(&self.name)));
            cells.push(format!("text(9pt, {})", quoted(&self.details)));
            let columns = if self.logo.is_empty() { "(1fr, auto)" } else { "(auto, 1fr, auto)" };
            let grid = format!("grid(columns: {columns}, column-gutter: 8pt, align: horizon, {})", cells.join(", "));
            parts.push(format!("  header: {},", once(format!("[#{grid} #v(-4pt) #line(length: 100%, stroke: 0.5pt)]"))));
            if !self.footer.is_empty() {
                parts.push(format!("  footer: {},", once(format!("align(center, text(8pt, {}))", quoted(&self.footer)))));
            }
        }
        if self.mark_on {
            let mark = if self.mark_is_image {
                self.faded_mark(preview)
            } else {
                format!("text({}pt, fill: luma({}), {})", self.mark_size.round(), self.mark_shade.round(), quoted(&self.mark_text))
            };
            parts.push(format!("  background: rotate({}deg, {mark}),", self.mark_angle.round()));
        }
        (!parts.is_empty()).then(|| format!("#set page(\n{}\n)", parts.join("\n")))
    }

    /// A whole small document to look at.
    pub fn preview(&self, sample: &str) -> String {
        format!("#set page(paper: \"a4\", margin: 2.5cm)\n{}\n{}\n#lorem(90)\n", self.rule(true).unwrap_or_default(), quoted_content(sample))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "head_on": self.head_on, "name": self.name, "details": self.details, "footer": self.footer, "logo": self.logo,
            "first_page_only": self.first_page_only, "mark_on": self.mark_on, "mark_is_image": self.mark_is_image,
            "mark_text": self.mark_text, "mark_image": self.mark_image, "mark_angle": self.mark_angle,
            "mark_size": self.mark_size, "mark_shade": self.mark_shade, "mark_opacity": self.mark_opacity,
        })
    }

    pub fn from_json(value: &Value) -> Self {
        const MAX_TEXT: usize = 2000;
        let base = Self::default();
        let text = |key: &str| value.get(key).and_then(Value::as_str).map_or_else(String::new, |s| s.chars().take(MAX_TEXT).collect());
        let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        let number = |key: &str, default: f32, range: std::ops::RangeInclusive<f32>| {
            value.get(key).and_then(Value::as_f64).map_or(default, |n| (n as f32).clamp(*range.start(), *range.end()))
        };
        Self {
            head_on: flag("head_on"),
            name: text("name"),
            details: text("details"),
            footer: text("footer"),
            logo: text("logo"),
            first_page_only: flag("first_page_only"),
            mark_on: flag("mark_on"),
            mark_is_image: flag("mark_is_image"),
            mark_text: text("mark_text"),
            mark_image: text("mark_image"),
            mark_angle: number("mark_angle", base.mark_angle, ANGLES),
            mark_size: number("mark_size", base.mark_size, SIZES),
            mark_shade: number("mark_shade", base.mark_shade, SHADES),
            // A document from before there was a choice keeps the picture as strong as it was.
            mark_opacity: number("mark_opacity", *OPACITIES.end(), OPACITIES),
        }
    }
}

fn quoted_content(text: &str) -> String {
    format!("#text({})", quoted(text))
}

/// Where the block is in a document, with the settings in it.
fn find_block(text: &str) -> Option<(Range<usize>, Letterhead)> {
    let start = if text.starts_with(START) { 0 } else { text.find(&format!("\n{START}"))? + 1 };
    let first_end = text.get(start..)?.find('\n')? + start;
    let settings = text.get(start + START.len()..first_end)?;
    let value: Value = serde_json::from_str(settings).ok()?;
    let close = text.get(first_end..)?.find(&format!("\n{END}"))? + first_end + 1 + END.len();
    let end = if text.get(close..)?.starts_with('\n') { close + 1 } else { close };
    Some((start..end, Letterhead::from_json(&value)))
}

/// The settings in a document, if it has them.
pub fn in_document(text: &str) -> Option<Letterhead> {
    find_block(text).map(|(_, settings)| settings)
}

/// Writes these settings at the top of the document, in place of those it has, or takes them out when
/// nothing's on. Whatever else the document says stays as it is.
pub fn apply(text: &mut String, sel: Sel, settings: &Letterhead) -> Sel {
    let block = settings.rule(false).map(|rule| format!("{START}{}\n{rule}\n{END}\n", settings.to_json()));
    let range = find_block(text).map_or(0..0, |(range, _)| range);
    edit::replace_keeping(text, sel, vec![(range, block.unwrap_or_default())])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Letterhead {
        Letterhead {
            head_on: true,
            name: "Empresa \"X\" #1".to_string(),
            details: "Rua A, 1".to_string(),
            footer: "contato@exemplo.com".to_string(),
            logo: "images/logo.png".to_string(),
            mark_on: true,
            mark_text: "RASCUNHO".to_string(),
            ..Letterhead::default()
        }
    }

    #[test]
    fn what_is_typed_stays_text() {
        let rule = full().rule(false).unwrap();
        assert!(rule.contains(r##""Empresa \"X\" #1""##), "{rule}");
        assert!(rule.contains("image(\"images/logo.png\", height: 1cm)"), "{rule}");
        assert!(rule.contains("rotate(-45deg, text(70pt, fill: luma(225), \"RASCUNHO\"))"), "{rule}");
        assert!(full().rule(true).unwrap().contains("rect("), "a preview reads no files");
        assert_eq!(Letterhead::default().rule(false), None);
    }

    #[test]
    fn a_picture_fades_by_its_opacity() {
        let mark = Letterhead { mark_on: true, mark_is_image: true, mark_image: "images/selo.png".to_string(), ..Letterhead::default() };
        let rule = mark.rule(false).unwrap();
        assert!(rule.contains("image(\"images/selo.png\", width: 0.7 * size.width)"), "{rule}");
        assert!(rule.contains("paper.transparentize(25%)"), "{rule}");

        let whole = Letterhead { mark_opacity: 100.0, ..mark.clone() }.rule(false).unwrap();
        assert!(whole.contains("image(\"images/selo.png\", width: 70%)") && !whole.contains("transparentize"), "{whole}");

        assert!(mark.rule(true).unwrap().contains("luma(120).transparentize(75%)"), "the preview follows it");
    }

    #[test]
    fn the_opacity_is_kept_and_hostile_or_missing_values_are_survived() {
        let mark = Letterhead { mark_opacity: 40.0, ..Letterhead::default() };
        assert_eq!(Letterhead::from_json(&mark.to_json()).mark_opacity, 40.0);
        assert_eq!(Letterhead::from_json(&json!({})).mark_opacity, 100.0, "older documents stay as they were");
        assert_eq!(Letterhead::from_json(&json!({"mark_opacity": -1e30})).mark_opacity, *OPACITIES.start());
        assert_eq!(Letterhead::from_json(&json!({"mark_opacity": "x"})).mark_opacity, 100.0);
    }

    #[test]
    fn the_block_is_written_replaced_and_taken_out() {
        let mut text = "= Título\nTexto\n".to_string();
        let caret = text.chars().count();
        let sel = apply(&mut text, (caret, caret), &full());
        assert!(text.starts_with(START) && text.ends_with("= Título\nTexto\n"), "{text}");
        assert_eq!(sel.0, text.chars().count(), "the caret stays on its letter");
        assert_eq!(in_document(&text), Some(full()));

        let mut changed = full();
        changed.mark_text = "CÓPIA".to_string();
        apply(&mut text, (0, 0), &changed);
        assert_eq!(text.matches(START).count(), 1);
        assert_eq!(in_document(&text).unwrap().mark_text, "CÓPIA");

        apply(&mut text, (0, 0), &Letterhead::default());
        assert_eq!(text, "= Título\nTexto\n");
        assert_eq!(in_document(&text), None);
    }

    #[test]
    fn a_tampered_comment_is_survived() {
        for text in [
            "// pearl:letterhead \n#set page()\n// pearl:end\n",
            "// pearl:letterhead {\n// pearl:end\n",
            "// pearl:letterhead {}",
            "// pearl:letterhead []\n// pearl:end",
            "// pearl:letterhead {\"mark_size\":1e30,\"mark_angle\":\"x\",\"name\":3}\n// pearl:end\n",
        ] {
            let found = in_document(text);
            if let Some(settings) = found {
                assert!(SIZES.contains(&settings.mark_size));
                assert_eq!(settings.name, "");
            }
            let mut buffer = text.to_string();
            apply(&mut buffer, (0, 0), &full());
        }
        assert_eq!(Letterhead::from_json(&json!({"mark_size": 1e30})).mark_size, *SIZES.end());
    }
}
