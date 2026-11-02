//! A passage from the past: what a past version says and the document no longer does, and how to bring
//! it back without touching the rest. Blocks come from Typst's parser, so a table or a code block with
//! blank lines in it stays whole. Past versions are hostile input like any document.

use std::collections::HashSet;
use std::ops::Range;

use typst::syntax::{SyntaxKind, parse};

use crate::edit::{self, Sel};

/// A version that has lost more than this has been rewritten, and the list would help nobody.
const MOST_PASSAGES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Block {
    range: Range<usize>,
    /// Rules and definitions: places to anchor by, but not passages to offer.
    rule: bool,
    /// It has more than comments and spaces.
    words: bool,
}

/// A document's top level, in blocks, by paragraph breaks and what stands on its own.
fn blocks(text: &str) -> Vec<Block> {
    let root = parse(text);
    let mut found = Vec::new();
    let (mut start, mut offset, mut words) = (0, 0, false);
    let mut hash = None;
    let flush = |found: &mut Vec<Block>, range: Range<usize>, words: bool| {
        if !range.is_empty() {
            found.push(Block { range, rule: false, words });
        }
    };
    for child in root.children() {
        let end = offset + child.len();
        match child.kind() {
            SyntaxKind::Parbreak => {
                flush(&mut found, start..offset, words);
                (start, words, hash) = (end, false, None);
            }
            SyntaxKind::Heading | SyntaxKind::ListItem | SyntaxKind::EnumItem | SyntaxKind::TermItem => {
                flush(&mut found, start..offset, words);
                found.push(Block { range: offset..end, rule: false, words: true });
                (start, words, hash) = (end, false, None);
            }
            SyntaxKind::SetRule | SyntaxKind::ShowRule | SyntaxKind::LetBinding | SyntaxKind::ModuleImport => {
                let from = hash.unwrap_or(offset);
                flush(&mut found, start..from, words);
                found.push(Block { range: from..end, rule: true, words: true });
                (start, words, hash) = (end, false, None);
            }
            SyntaxKind::Hash => {
                hash = Some(offset);
                words = true;
            }
            SyntaxKind::Space | SyntaxKind::LineComment | SyntaxKind::BlockComment => {}
            _ => {
                hash = None;
                words = true;
            }
        }
        offset = end;
    }
    flush(&mut found, start..offset, words);
    // Spaces around a block aren't part of it.
    for block in &mut found {
        let inner = text.get(block.range.clone()).unwrap_or_default();
        let trimmed = inner.trim_start();
        let from = block.range.start + (inner.len() - trimmed.len());
        block.range = from..from + trimmed.trim_end().len();
    }
    found.retain(|block| !block.range.is_empty());
    found
}

/// What a past version says that the document doesn't, with the blocks around it that it still has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    pub text: String,
    /// The block it came after, if the document still has it.
    after: Option<String>,
    /// The block it came before, likewise.
    before: Option<String>,
}

/// The passages of `old` that aren't in `now`, in order.
pub fn lost_passages(old: &str, now: &str) -> Vec<Passage> {
    let piece = |text: &'_ str, block: &Block| text.get(block.range.clone()).unwrap_or_default().to_string();
    let current: HashSet<String> = blocks(now).iter().map(|block| piece(now, block)).collect();
    let past = blocks(old);
    let kept = |block: &Block| current.contains(&piece(old, block));
    let mut lost = Vec::new();
    for (index, block) in past.iter().enumerate() {
        if lost.len() >= MOST_PASSAGES {
            break;
        }
        if block.rule || !block.words || kept(block) {
            continue;
        }
        let text = piece(old, block);
        // Run into a longer paragraph, it isn't lost.
        if now.contains(&text) {
            continue;
        }
        let after = past.get(..index).and_then(|before| before.iter().rev().find(|b| kept(b))).map(|b| piece(old, b));
        let before = past.get(index + 1..).and_then(|rest| rest.iter().find(|b| kept(b))).map(|b| piece(old, b));
        lost.push(Passage { text, after, before });
    }
    lost
}

/// Puts a passage back where it was, as far as what's around it still says, and changes nothing else.
pub fn bring_back(text: &mut String, sel: Sel, passage: &Passage) -> Sel {
    let present = blocks(text);
    let place = |wanted: &Option<String>| {
        let wanted = wanted.as_deref()?;
        present.iter().find(|block| text.get(block.range.clone()) == Some(wanted))
    };
    let change = if let Some(block) = place(&passage.after) {
        (block.range.end..block.range.end, format!("\n\n{}", passage.text))
    } else if let Some(block) = place(&passage.before) {
        (block.range.start..block.range.start, format!("{}\n\n", passage.text))
    } else {
        // Nothing of what was around it is left: at the end, out of the way.
        let end = text.trim_end().len();
        let gap = if end == 0 { "" } else { "\n\n" };
        (end..end, format!("{gap}{}", passage.text))
    };
    edit::replace_keeping(text, sel, vec![change])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(old: &str, now: &str) -> Vec<String> {
        lost_passages(old, now).into_iter().map(|p| p.text).collect()
    }

    const OLD: &str = "#set text(size: 11pt)\n\n= Título\nUm.\n\nDois.\n\nTrês.\n";

    #[test]
    fn what_the_document_no_longer_says_is_found() {
        assert_eq!(texts(OLD, OLD), Vec::<String>::new());
        assert_eq!(texts(OLD, "#set text(size: 11pt)\n\n= Título\nUm.\n\nTrês.\n"), ["Dois."]);
        assert_eq!(texts(OLD, "#set text(size: 10pt)\n\n= Título\nUm.\n\nDois.\n\nTrês.\n"), Vec::<String>::new(), "a changed rule is no passage");
        assert_eq!(texts(OLD, "= Título\nUm. Dois.\n\nTrês.\n"), Vec::<String>::new(), "run into another paragraph, it's still there");
    }

    #[test]
    fn a_block_stays_whole_and_comments_are_no_passage() {
        let table = "#table(\n  [a],\n\n  [b],\n)";
        assert_eq!(texts(&format!("Um.\n\n{table}\n\nDois.\n"), "Um.\n\nDois.\n"), [table]);
        assert_eq!(texts("// nota\n\nUm.\n", "Um.\n"), Vec::<String>::new());
        assert_eq!(texts("- a\n- b\n", "- a\n"), ["- b"]);
    }

    #[test]
    fn a_passage_goes_back_to_its_place_and_nothing_else_changes() {
        let now = "#set text(size: 11pt)\n\n= Título\nUm.\n\nTrês.\n";
        let passages = lost_passages(OLD, now);
        assert_eq!(passages.len(), 1);
        let mut text = now.to_string();
        bring_back(&mut text, (0, 0), &passages[0]);
        assert_eq!(text, "#set text(size: 11pt)\n\n= Título\nUm.\n\nDois.\n\nTrês.\n");
        assert_eq!(lost_passages(OLD, &text), []);
    }

    #[test]
    fn with_what_was_before_it_gone_it_goes_before_what_was_after_it() {
        let passages = lost_passages("Um.\n\nDois.\n\nTrês.\n", "Três.\n");
        let mut text = "Três.\n".to_string();
        bring_back(&mut text, (0, 0), &passages[1]);
        assert_eq!(text, "Dois.\n\nTrês.\n");
    }

    #[test]
    fn with_all_around_it_gone_it_goes_to_the_end() {
        let passages = lost_passages("Um.\n", "Outra coisa.\n");
        let mut text = "Outra coisa.\n".to_string();
        bring_back(&mut text, (0, 0), &passages[0]);
        assert_eq!(text, "Outra coisa.\n\nUm.\n");
        let mut empty = String::new();
        bring_back(&mut empty, (0, 0), &passages[0]);
        assert_eq!(empty, "Um.");
    }

    #[test]
    fn the_caret_stays_on_its_letter() {
        let now = "Um.\n\nTrês.\n";
        let passages = lost_passages("Um.\n\nDois.\n\nTrês.\n", now);
        let mut text = now.to_string();
        let at = now.find("Três").unwrap();
        let (a, _) = bring_back(&mut text, (at, at), &passages[0]);
        assert_eq!(text.get(a..a + 2), Some("Tr"));
    }

    #[test]
    fn hostile_versions_are_survived() {
        for old in ["", "\n\n\n", "#", "#set", "= ", "#[", "$", "```", "#table(", "é\n\n#\n\nç", "\u{0}\n\n\u{feff}"] {
            for passage in lost_passages(old, "Agora.\n") {
                let mut text = "Agora.\n".to_string();
                bring_back(&mut text, (0, 0), &passage);
            }
            assert!(lost_passages("Agora.\n", old).len() <= 1);
        }
        let many: String = (0..1000).map(|n| format!("Parágrafo {n}.\n\n")).collect();
        assert_eq!(lost_passages(&many, "").len(), MOST_PASSAGES);
    }
}
