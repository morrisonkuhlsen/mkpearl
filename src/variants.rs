//! Layout variants: a few other ways to set a document. A variant is the edit it would make to the source,
//! so what's previewed is exactly what choosing it writes.

use crate::edit::{self, Sel};

/// A way to set a document. What it leaves as `None` stays as the document has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    /// The id of its name in the catalogs.
    pub name: &'static str,
    pub margin: Option<&'static str>,
    pub size: Option<&'static str>,
    pub leading: Option<&'static str>,
    pub font: Option<&'static str>,
}

const NOTHING: Variant = Variant { name: "variant-current", margin: None, size: None, leading: None, font: None };

/// The first is the document as it is. The font ships with Typst, so it's there on every machine.
pub const PRESETS: [Variant; 6] = [
    NOTHING,
    Variant { name: "variant-compact", margin: Some("2cm"), size: Some("10pt"), leading: Some("0.55em"), ..NOTHING },
    Variant { name: "variant-narrow-margins", margin: Some("1.5cm"), ..NOTHING },
    Variant { name: "variant-comfortable", margin: Some("3cm"), size: Some("12pt"), leading: Some("0.8em"), ..NOTHING },
    Variant { name: "variant-open-lines", leading: Some("1.05em"), ..NOTHING },
    Variant { name: "variant-classic", font: Some("\"New Computer Modern\""), ..NOTHING },
];

impl Variant {
    /// Writes this variant in the source, keeping the selection on the same letters.
    pub fn apply(&self, text: &mut String, mut sel: Sel) -> Sel {
        if let Some(value) = self.margin {
            sel = edit::set_page_margin(text, sel, value);
        }
        for (function, argument, value) in [("text", "size", self.size), ("par", "leading", self.leading), ("text", "font", self.font)] {
            if let Some(value) = value {
                sel = edit::set_argument(text, sel, function, argument, value);
            }
        }
        sel
    }

    /// The source with this variant written in it.
    pub fn applied_to(&self, text: &str) -> String {
        let mut out = text.to_string();
        self.apply(&mut out, (0, 0));
        out
    }

    pub fn changes_nothing(&self) -> bool {
        self.margin.is_none() && self.size.is_none() && self.leading.is_none() && self.font.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_preset_leaves_the_document_as_it_is() {
        assert!(PRESETS[0].changes_nothing());
        assert!(PRESETS[1..].iter().all(|v| !v.changes_nothing()));
        assert_eq!(PRESETS[0].applied_to("= Título\n"), "= Título\n");
    }

    #[test]
    fn a_value_is_changed_where_the_document_already_says_it() {
        let text = "#set page(paper: \"a4\", margin: 2.5cm)\n#set text(lang: \"pt\", size: 11pt)\n#set par(justify: true, leading: 1em)\n= Título\n";
        let done = PRESETS[1].applied_to(text);
        assert_eq!(
            done,
            "#set page(paper: \"a4\", margin: 2cm)\n#set text(lang: \"pt\", size: 10pt)\n#set par(justify: true, leading: 0.55em)\n= Título\n"
        );
    }

    #[test]
    fn what_a_document_doesnt_say_is_added_before_it() {
        let done = PRESETS[5].applied_to("#set text(lang: \"pt\")\n= Título\n");
        assert_eq!(done, "#set text(font: \"New Computer Modern\")\n#set text(lang: \"pt\")\n= Título\n");
    }

    #[test]
    fn a_rule_inside_a_passage_is_not_taken_for_the_documents() {
        let text = "Antes\n#[#set text(size: 8pt)\nPequeno]\n";
        assert_eq!(PRESETS[1].applied_to(text).matches("size: 8pt").count(), 1, "the passage keeps its size");
        assert!(PRESETS[1].applied_to(text).contains("#set text(size: 10pt)\n"));
    }

    #[test]
    fn the_selection_stays_on_the_same_letters() {
        let mut text = "#set text(size: 11pt)\nOlá mundo\n".to_string();
        let at = text.chars().count() - "mundo\n".chars().count();
        let (a, b) = PRESETS[1].apply(&mut text, (at, at));
        assert_eq!((a, b), (text.chars().count() - "mundo\n".chars().count(), text.chars().count() - "mundo\n".chars().count()));
    }
}
