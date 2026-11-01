//! Typing helped along: brackets, quotes and `$` close themselves, and typing the closer steps over
//! the one already there. Works on text and char offsets, with Typst's parser saying what the caret
//! is inside.

use typst::syntax::{LinkedNode, Side, SyntaxKind, parse};

use crate::edit::{self, Sel};

fn closer(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '$' => Some('$'),
        _ => None,
    }
}

/// Whether the typing help would do anything with this character.
pub fn is_special(c: char) -> bool {
    closer(c).is_some() || matches!(c, ')' | ']' | '}')
}

#[derive(PartialEq)]
enum Place {
    Normal,
    /// In a string, where only the closing quote means anything.
    String,
    /// In a comment or raw text, where nothing pairs.
    Inert,
}

fn place_of(text: &str, byte: usize) -> Place {
    let root = parse(text);
    let Some(leaf) = LinkedNode::new(&root).leaf_at(byte, Side::Before) else { return Place::Normal };
    let closed_here = leaf.range().end == byte;
    match leaf.kind() {
        SyntaxKind::Str => {
            let body = text.get(leaf.range()).unwrap_or_default();
            // A finished string ending at the caret is behind it, not around it.
            if closed_here && body.len() >= 2 && body.ends_with('"') { Place::Normal } else { Place::String }
        }
        SyntaxKind::LineComment => Place::Inert,
        SyntaxKind::BlockComment if !closed_here => Place::Inert,
        // Anywhere in raw text but right after its closing delimiter.
        kind if leaf.parent_kind() == Some(SyntaxKind::Raw) => {
            let finished = kind == SyntaxKind::RawDelim && closed_here && leaf.next_sibling().is_none();
            if finished { Place::Normal } else { Place::Inert }
        }
        _ => Place::Normal,
    }
}

/// Types `c` at the selection, helped. `None` when it should just go in as typed.
pub fn type_char(text: &mut String, (a, b): Sel, c: char) -> Option<Sel> {
    let (start, end) = (edit::byte_of(text, a), edit::byte_of(text, b));
    let next = text.get(end..).and_then(|t| t.chars().next());
    let previous = text.get(..start).and_then(|t| t.chars().next_back());
    let place = place_of(text, start);

    // Typing the closer that's already next steps over it.
    if a == b && next == Some(c) && matches!(c, ')' | ']' | '}' | '"' | '$') && (place != Place::Inert) && (c == '"' || place != Place::String) {
        return Some((a + 1, a + 1));
    }
    let close = closer(c)?;
    if place != Place::Normal {
        return None;
    }
    // A selection gets wrapped, and stays selected.
    if a != b {
        text.insert(end, close);
        text.insert(start, c);
        return Some((a + 1, b + 1));
    }
    // A quote or `$` right after a word is punctuation or a price, not an opening.
    if matches!(c, '"' | '$') && previous.is_some_and(char::is_alphanumeric) {
        return None;
    }
    // Before a word, the bracket is meant for that word: the closer belongs further on.
    if next.is_some_and(|n| !n.is_whitespace() && !matches!(n, ')' | ']' | '}' | ',' | ';' | '.' | ':' | '$' | '"')) {
        return None;
    }
    text.insert(start, close);
    text.insert(start, c);
    Some((a + 1, a + 1))
}

/// Backspace between an empty pair takes both. `None` when it should do what it always does.
pub fn backspace_pair(text: &mut String, (a, b): Sel) -> Option<Sel> {
    if a != b || a == 0 {
        return None;
    }
    let at = edit::byte_of(text, a);
    let before = text.get(..at)?.chars().next_back()?;
    let after = text.get(at..)?.chars().next()?;
    if closer(before) != Some(after) {
        return None;
    }
    text.replace_range(at - before.len_utf8()..at + after.len_utf8(), "");
    Some((a - 1, a - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `c` at the `|` in `before`, returning the text with `|` at the new caret (or `None`).
    fn typed(before: &str, c: char) -> Option<String> {
        let at = before.find('|').unwrap();
        let mut text = before.replacen('|', "", 1);
        let chars = text.get(..at).unwrap().chars().count();
        let (caret, _) = type_char(&mut text, (chars, chars), c)?;
        let byte = edit::byte_of(&text, caret);
        text.insert(byte, '|');
        Some(text)
    }

    #[test]
    fn brackets_close_themselves_with_the_caret_inside() {
        assert_eq!(typed("#set text|", '(').as_deref(), Some("#set text(|)"));
        assert_eq!(typed("#figure(|)", '[').as_deref(), Some("#figure([|])"));
        assert_eq!(typed("#let f() = |", '{').as_deref(), Some("#let f() = {|}"));
        assert_eq!(typed("ação #f|", '(').as_deref(), Some("ação #f(|)"));
    }

    #[test]
    fn the_closer_steps_over_the_one_that_is_there() {
        assert_eq!(typed("#f(|)", ')').as_deref(), Some("#f()|"));
        assert_eq!(typed("#f([|])", ']').as_deref(), Some("#f([]|)"));
        assert_eq!(typed("#f(\"a|\")", '"').as_deref(), Some("#f(\"a\"|)"));
        assert_eq!(typed("$ x|$", '$').as_deref(), Some("$ x$|"));
    }

    #[test]
    fn a_closer_with_nothing_to_step_over_goes_in_as_typed() {
        assert_eq!(typed("#f(|", ')'), None);
    }

    #[test]
    fn before_a_word_a_bracket_does_not_pair() {
        assert_eq!(typed("|palavra", '('), None);
        assert_eq!(typed("|palavra", '['), None);
        assert_eq!(typed("|\n", '(').as_deref(), Some("(|)\n"));
    }

    #[test]
    fn quotes_and_dollars_after_a_word_are_just_typed() {
        assert_eq!(typed("R|", '$'), None);
        assert_eq!(typed("polegada|", '"'), None);
        assert_eq!(typed("a |", '$').as_deref(), Some("a $|$"));
        assert_eq!(typed("#image(|)", '"').as_deref(), Some("#image(\"|\")"));
    }

    #[test]
    fn inside_strings_and_comments_nothing_pairs() {
        assert_eq!(typed("#f(\"a|\")", '('), None);
        assert_eq!(typed("// nota |", '('), None);
        assert_eq!(typed("// nota |", '"'), None);
        assert_eq!(typed("```\ncódigo |\n```", '('), None);
    }

    #[test]
    fn after_finished_raw_text_pairing_works_again() {
        assert_eq!(typed("`a`|", '(').as_deref(), Some("`a`(|)"));
    }

    #[test]
    fn after_a_finished_string_pairing_works_again() {
        assert_eq!(typed("#f(\"a\"|", '[').as_deref(), Some("#f(\"a\"[|]"));
    }

    #[test]
    fn a_selection_gets_wrapped_and_stays_selected() {
        let mut t = "é ação".to_string();
        assert_eq!(type_char(&mut t, (2, 6), '('), Some((3, 7)));
        assert_eq!(t, "é (ação)");
    }

    #[test]
    fn backspace_takes_an_empty_pair_only() {
        for (text, caret, left) in [("()", 1, Some("")), ("é[]x", 2, Some("éx")), ("\"\"", 1, Some("")), ("(a)", 1, None), ("(a)", 2, None), ("()", 0, None), ("()", 2, None)] {
            let mut t = text.to_string();
            let done = backspace_pair(&mut t, (caret, caret));
            match left {
                Some(rest) => {
                    assert_eq!(done, Some((caret - 1, caret - 1)), "{text}");
                    assert_eq!(t, rest);
                }
                None => {
                    assert_eq!(done, None, "{text} at {caret}");
                    assert_eq!(t, text);
                }
            }
        }
        let mut t = "()".to_string();
        assert_eq!(backspace_pair(&mut t, (0, 2)), None, "a selection deletes as usual");
    }

    #[test]
    fn hostile_offsets_do_not_panic() {
        let mut t = "ação".to_string();
        let _ = type_char(&mut t, (99, 99), '(');
        let _ = backspace_pair(&mut t, (99, 99));
        let _ = type_char(&mut String::new(), (0, 0), ')');
    }
}
