//! Several carets at once: where they are and what typing does at each. Works on text and char
//! offsets, as `edit` does; the window is in `app/multi_caret`.

use crate::edit::{self, Sel};

/// A caret per line of a long document is as far as this goes; every keystroke visits them all.
pub const MAX_CARETS: usize = 1000;
/// Closing brackets asks Typst's parser about each caret, which is too slow for very many.
const MAX_HELPED: usize = 64;

/// One caret, with the selection from `anchor` to `head`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caret {
    pub anchor: usize,
    pub head: usize,
    /// The column going up and down aims for, past lines too short to reach it.
    column: Option<usize>,
}

impl Caret {
    pub fn at(place: usize) -> Caret {
        Caret::between(place, place)
    }

    pub fn between(anchor: usize, head: usize) -> Caret {
        Caret { anchor, head, column: None }
    }

    /// Where it starts and ends, in order.
    pub fn sel(self) -> Sel {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    fn is_empty(self) -> bool {
        self.anchor == self.head
    }
}

/// A way of moving the carets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
}

/// The lines of a text: each one's first char and its length, without the line break.
struct Lines {
    spans: Vec<(usize, usize)>,
}

impl Lines {
    fn of(chars: &[char]) -> Lines {
        let mut spans = Vec::new();
        let mut start = 0;
        for (i, c) in chars.iter().enumerate() {
            if *c == '\n' {
                spans.push((start, i - start));
                start = i + 1;
            }
        }
        spans.push((start, chars.len() - start));
        Lines { spans }
    }

    /// The line a place is on, and its column there.
    fn place(&self, at: usize) -> (usize, usize) {
        let line = self.spans.partition_point(|(start, _)| *start <= at).saturating_sub(1);
        let start = self.spans.get(line).map_or(0, |s| s.0);
        (line, at.saturating_sub(start))
    }

    /// The place at a line and column, or the line's end when it's shorter.
    fn at(&self, line: usize, column: usize) -> Option<usize> {
        self.spans.get(line).map(|(start, len)| start + column.min(*len))
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_left(chars: &[char], from: usize) -> usize {
    let mut at = from.min(chars.len());
    let before = |at: usize| at.checked_sub(1).and_then(|i| chars.get(i)).copied();
    while before(at).is_some_and(char::is_whitespace) {
        at -= 1;
    }
    let word = before(at).is_some_and(is_word);
    while before(at).is_some_and(|c| !c.is_whitespace() && is_word(c) == word) {
        at -= 1;
    }
    at
}

fn word_right(chars: &[char], from: usize) -> usize {
    let mut at = from.min(chars.len());
    let next = |at: usize| chars.get(at).copied();
    while next(at).is_some_and(char::is_whitespace) {
        at += 1;
    }
    let word = next(at).is_some_and(is_word);
    while next(at).is_some_and(|c| !c.is_whitespace() && is_word(c) == word) {
        at += 1;
    }
    at
}

fn delete(text: &mut String, (a, b): Sel) -> Sel {
    let (from, to) = (edit::byte_of(text, a), edit::byte_of(text, b));
    text.replace_range(from..to, "");
    (a, a)
}

/// All the carets, in the order of the text and never overlapping, with one of them the main one:
/// the last placed, which the editor itself shows and scrolls to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carets {
    list: Vec<Caret>,
    primary: usize,
}

impl Carets {
    /// The one caret there is before any is added.
    pub fn one((a, b): Sel) -> Carets {
        Carets { list: vec![Caret::between(a, b)], primary: 0 }
    }

    pub fn list(&self) -> &[Caret] {
        &self.list
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn primary(&self) -> Caret {
        self.list.get(self.primary).copied().unwrap_or_else(|| Caret::at(0))
    }

    pub fn primary_index(&self) -> usize {
        self.primary
    }

    /// Replaces the main caret, as dragging it does.
    pub fn set_primary(&mut self, caret: Caret) {
        let mut list = self.list.clone();
        if self.primary < list.len() {
            list.remove(self.primary);
        }
        self.rebuild(list, Some(caret));
    }

    /// Adds a caret at a place, which becomes the main one, or takes away the one already there as
    /// long as another stays.
    pub fn toggle(&mut self, place: usize) {
        let there = self.list.iter().position(|c| {
            let (a, b) = c.sel();
            a <= place && place <= b
        });
        match there {
            Some(i) if self.list.len() > 1 => {
                self.list.remove(i);
                self.primary = self.list.len() - 1;
            }
            Some(_) => {}
            None => self.add(Caret::at(place)),
        }
    }

    fn add(&mut self, caret: Caret) {
        if self.list.len() < MAX_CARETS {
            let list = self.list.clone();
            self.rebuild(list, Some(caret));
        }
    }

    /// Puts a list in order and joins what overlaps; `newest`, if any, becomes the main one, and
    /// otherwise the main one stays.
    fn rebuild(&mut self, list: Vec<Caret>, newest: Option<Caret>) {
        let primary = self.primary;
        let mut marked: Vec<(Caret, bool)> = list.into_iter().enumerate().map(|(i, c)| (c, newest.is_none() && i == primary)).collect();
        if let Some(caret) = newest {
            marked.push((caret, true));
        }
        self.settle(marked);
    }

    fn settle(&mut self, mut marked: Vec<(Caret, bool)>) {
        marked.sort_by_key(|(c, _)| c.sel());
        let mut out: Vec<(Caret, bool)> = Vec::with_capacity(marked.len());
        for (caret, main) in marked {
            let (a, b) = caret.sel();
            match out.last_mut() {
                Some((last, last_main)) if overlap(last.sel(), (a, b)) => {
                    let (from, to) = last.sel();
                    let to = to.max(b);
                    let backwards = if main { caret.head < caret.anchor } else { last.head < last.anchor };
                    let column = if main { caret.column } else { last.column };
                    *last = if backwards { Caret { anchor: to, head: from, column } } else { Caret { anchor: from, head: to, column } };
                    *last_main |= main;
                }
                _ => out.push((caret, main)),
            }
        }
        self.primary = out.iter().position(|(_, main)| *main).unwrap_or(out.len().saturating_sub(1));
        self.list = out.into_iter().map(|(c, _)| c).collect();
    }

    /// Runs an edit at each caret, from the last to the first so the places yet to come stay right.
    /// `edit` gets the caret's selection and its position in the list, and says where it ends.
    fn each(&mut self, text: &mut String, mut edit: impl FnMut(&mut String, Sel, usize) -> Sel) {
        let mut done: Vec<(Caret, bool)> = Vec::with_capacity(self.list.len());
        let mut size = text.chars().count();
        for (i, caret) in self.list.iter().enumerate().rev() {
            let (a, b) = edit(text, caret.sel(), i);
            let now = text.chars().count();
            for (later, _) in &mut done {
                later.anchor = (later.anchor + now).saturating_sub(size);
                later.head = (later.head + now).saturating_sub(size);
            }
            size = now;
            done.push((Caret::between(a, b), i == self.primary));
        }
        self.settle(done);
    }

    /// Types at every caret, in place of its selection.
    pub fn insert(&mut self, text: &mut String, piece: &str) {
        self.each(text, |text, sel, _| edit::insert_plain(text, sel, piece));
    }

    /// Types one character at every caret, with brackets closing themselves as they do with one.
    pub fn type_char(&mut self, text: &mut String, c: char) {
        let helped = self.list.len() <= MAX_HELPED && crate::typing::is_special(c);
        let mut buffer = [0; 4];
        let piece: &str = c.encode_utf8(&mut buffer);
        self.each(text, |text, sel, _| match helped.then(|| crate::typing::type_char(text, sel, c)).flatten() {
            Some(next) => next,
            None => edit::insert_plain(text, sel, piece),
        });
    }

    /// Pastes: one line per caret when the lines and the carets are as many, the whole at each if not.
    pub fn paste(&mut self, text: &mut String, pasted: &str) {
        let lines: Vec<&str> = pasted.lines().collect();
        if lines.len() == self.list.len() {
            self.each(text, |text, sel, i| edit::insert_plain(text, sel, lines.get(i).copied().unwrap_or_default()));
        } else {
            self.insert(text, pasted);
        }
    }

    /// Takes away each selection or, where there's none, the character before the caret or the word.
    pub fn backspace(&mut self, text: &mut String, word: bool) {
        if word {
            // The ranges are worked out first: two carets in a word reach for the same characters.
            let chars: Vec<char> = text.chars().collect();
            let primary = self.primary;
            let ranges = self
                .list
                .iter()
                .enumerate()
                .map(|(i, caret)| {
                    let (a, b) = caret.sel();
                    (Caret::between(if a == b { word_left(&chars, a) } else { a }, b), i == primary)
                })
                .collect();
            self.settle(ranges);
            self.each(text, |text, sel, _| delete(text, sel));
            return;
        }
        let helped = self.list.len() <= MAX_HELPED;
        self.each(text, |text, (a, b), _| {
            if a != b {
                return delete(text, (a, b));
            }
            if helped && let Some(next) = crate::typing::backspace_pair(text, (a, b)) {
                return next;
            }
            delete(text, (a.saturating_sub(1), a))
        });
    }

    /// Takes away each selection or, where there's none, the character after the caret or the word.
    pub fn delete_forward(&mut self, text: &mut String, word: bool) {
        let chars: Vec<char> = text.chars().collect();
        // The ranges are worked out first: two carets side by side reach for the same characters.
        let mut ranges: Vec<(Caret, bool)> = Vec::with_capacity(self.list.len());
        for (i, caret) in self.list.iter().enumerate() {
            let (a, b) = caret.sel();
            let to = if a != b {
                b
            } else if word {
                word_right(&chars, a)
            } else {
                (a + 1).min(chars.len())
            };
            ranges.push((Caret::between(a, to), i == self.primary));
        }
        self.settle(ranges);
        self.each(text, |text, sel, _| delete(text, sel));
    }

    /// What copying takes: the selections, a line each; with none, the lines the carets are on.
    pub fn copied(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let slice = |(a, b): Sel| chars.get(a..b).map_or_else(String::new, |s| s.iter().collect::<String>());
        if self.list.iter().all(|c| c.is_empty()) {
            self.whole_lines(&chars).list.iter().map(|c| slice(c.sel())).map(|l| if l.ends_with('\n') { l } else { l + "\n" }).collect()
        } else {
            self.list.iter().filter(|c| !c.is_empty()).map(|c| slice(c.sel())).collect::<Vec<_>>().join("\n")
        }
    }

    /// Cuts what `copied` takes.
    pub fn cut(&mut self, text: &mut String) {
        if self.list.iter().all(|c| c.is_empty()) {
            let chars: Vec<char> = text.chars().collect();
            *self = self.whole_lines(&chars);
        }
        self.each(text, |text, sel, _| delete(text, sel));
    }

    /// The whole lines the carets are on, each once, with their line breaks.
    fn whole_lines(&self, chars: &[char]) -> Carets {
        let lines = Lines::of(chars);
        let mut out = Carets { list: Vec::new(), primary: self.primary };
        let marked = self
            .list
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let (line, _) = lines.place(c.head);
                let (start, len) = lines.spans.get(line).copied().unwrap_or((0, 0));
                (Caret::between(start, (start + len + 1).min(chars.len())), i == self.primary)
            })
            .collect();
        out.settle(marked);
        out
    }

    /// Moves every caret; `extend` keeps each anchor where it is, selecting.
    pub fn go(&mut self, text: &str, way: Move, extend: bool) {
        let chars: Vec<char> = text.chars().collect();
        let lines = Lines::of(&chars);
        let primary = self.primary;
        let marked = self
            .list
            .iter()
            .enumerate()
            .map(|(i, caret)| {
                let (a, b) = caret.sel();
                let (line, here) = lines.place(caret.head);
                let aim = caret.column.unwrap_or(here);
                let vertical = matches!(way, Move::Up | Move::Down);
                let head = match way {
                    // With a selection and no shift, an arrow goes to the selection's edge.
                    Move::Left if !extend && a != b => a,
                    Move::Right if !extend && a != b => b,
                    Move::Left => caret.head.saturating_sub(1),
                    Move::Right => (caret.head + 1).min(chars.len()),
                    Move::WordLeft => word_left(&chars, caret.head),
                    Move::WordRight => word_right(&chars, caret.head),
                    Move::Up => line.checked_sub(1).and_then(|l| lines.at(l, aim)).unwrap_or(0),
                    Move::Down => lines.at(line + 1, aim).unwrap_or(chars.len()),
                    Move::LineStart => {
                        let start = caret.head - here;
                        let indent = chars.get(start..).map_or(0, |rest| rest.iter().take_while(|c| **c == ' ' || **c == '\t').count());
                        let len = lines.spans.get(line).map_or(0, |s| s.1);
                        // The text's start first, the line's next, as a code editor goes.
                        if here == indent.min(len) { start } else { start + indent.min(len) }
                    }
                    Move::LineEnd => lines.at(line, usize::MAX).unwrap_or(caret.head),
                };
                let anchor = if extend { caret.anchor } else { head };
                (Caret { anchor, head, column: vertical.then_some(aim) }, i == primary)
            })
            .collect();
        self.settle(marked);
    }

    /// Adds a caret on the line above the first one or below the last, at the main caret's column.
    pub fn add_line(&mut self, text: &str, up: bool) {
        let chars: Vec<char> = text.chars().collect();
        let lines = Lines::of(&chars);
        let (Some(first), Some(last)) = (self.list.first(), self.list.last()) else { return };
        let main = self.primary();
        let column = main.column.unwrap_or(lines.place(main.head).1);
        let line = if up { lines.place(first.head).0.checked_sub(1) } else { Some(lines.place(last.head).0 + 1) };
        if let Some(place) = line.and_then(|l| lines.at(l, column)) {
            self.add(Caret { anchor: place, head: place, column: Some(column) });
        }
    }

    /// The carets of a box selection, from one place's line and column to another's. Lines too short
    /// to reach it have none, unless it's only a straight line of carets.
    pub fn column(text: &str, from: (usize, usize), to: (usize, usize)) -> Carets {
        let chars: Vec<char> = text.chars().collect();
        let lines = Lines::of(&chars);
        let (top, bottom) = (from.0.min(to.0), from.0.max(to.0));
        let left = from.1.min(to.1);
        let mut list = Vec::new();
        for line in (top..=bottom).take(MAX_CARETS) {
            let Some((_, len)) = lines.spans.get(line).copied() else { break };
            if from.1 != to.1 && len < left {
                continue;
            }
            if let (Some(anchor), Some(head)) = (lines.at(line, from.1), lines.at(line, to.1)) {
                list.push(Caret::between(anchor, head));
            }
        }
        if list.is_empty() {
            list.push(Caret::at(lines.at(to.0, to.1).unwrap_or(chars.len())));
        }
        let primary = if to.0 < from.0 { 0 } else { list.len() - 1 };
        Carets { list, primary }
    }

    /// A place's line and column.
    pub fn line_and_column(text: &str, place: usize) -> (usize, usize) {
        let chars: Vec<char> = text.chars().collect();
        Lines::of(&chars).place(place)
    }
}

fn overlap((a, b): Sel, (c, d): Sel) -> bool {
    c < b || (a, b) == (c, d) || (a == b && a == c) || (c == d && c == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(places: &[usize]) -> Carets {
        let mut carets = Carets::one((places[0], places[0]));
        for p in &places[1..] {
            carets.toggle(*p);
        }
        carets
    }

    fn places(carets: &Carets) -> Vec<usize> {
        carets.list().iter().map(|c| c.head).collect()
    }

    #[test]
    fn typing_goes_in_at_every_caret_and_they_stay_after_it() {
        let mut text = "um\ndois\ntres".to_string();
        let mut carets = at(&[0, 3, 8]);
        carets.insert(&mut text, "- ");
        assert_eq!(text, "- um\n- dois\n- tres");
        assert_eq!(places(&carets), vec![2, 7, 14]);
    }

    #[test]
    fn accents_count_as_one_character() {
        let mut text = "é\nção\nü".to_string();
        let mut carets = at(&[1, 5, 7]);
        carets.insert(&mut text, "!");
        assert_eq!(text, "é!\nção!\nü!");
        carets.backspace(&mut text, false);
        assert_eq!(text, "é\nção\nü");
        assert_eq!(places(&carets), vec![1, 5, 7]);
    }

    #[test]
    fn typing_replaces_each_selection() {
        let mut text = "abc abc".to_string();
        let mut carets = Carets::one((0, 3));
        carets.add(Caret::between(4, 7));
        carets.insert(&mut text, "x");
        assert_eq!(text, "x x");
        assert_eq!(places(&carets), vec![1, 3]);
    }

    #[test]
    fn brackets_close_at_every_caret() {
        let mut text = "#a\n#b\n".to_string();
        let mut carets = at(&[2, 5]);
        carets.type_char(&mut text, '(');
        assert_eq!(text, "#a()\n#b()\n");
        assert_eq!(places(&carets), vec![3, 8]);
        carets.backspace(&mut text, false);
        assert_eq!(text, "#a\n#b\n");
    }

    #[test]
    fn deleting_side_by_side_takes_each_character_once() {
        let mut text = "abcd".to_string();
        let mut carets = at(&[1, 2]);
        carets.backspace(&mut text, false);
        assert_eq!(text, "cd");
        assert_eq!(places(&carets), vec![0]);
        let mut text = "abcd".to_string();
        let mut carets = at(&[1, 2]);
        carets.delete_forward(&mut text, false);
        assert_eq!(text, "ad");
        assert_eq!(places(&carets), vec![1]);
    }

    #[test]
    fn the_ends_of_the_text_are_safe() {
        let mut text = "ab".to_string();
        let mut carets = at(&[0, 2]);
        carets.backspace(&mut text, false);
        assert_eq!(text, "a");
        let mut carets = at(&[0, 1]);
        carets.delete_forward(&mut text, false);
        assert_eq!(text, "");
        let mut carets_far = Carets::one((99, 99));
        carets_far.toggle(120);
        carets_far.backspace(&mut text, false);
        carets_far.delete_forward(&mut text, true);
        carets_far.go(&text, Move::Up, false);
        carets_far.go(&text, Move::LineStart, false);
        carets_far.go(&text, Move::LineEnd, false);
        carets_far.add_line(&text, true);
        carets_far.cut(&mut text);
        assert_eq!(text, "");
        carets.go(&text, Move::Down, true);
    }

    #[test]
    fn word_keys_stop_at_words() {
        let mut text = "um dois tres\nquatro cinco".to_string();
        let mut carets = at(&[7, 19]);
        carets.backspace(&mut text, true);
        assert_eq!(text, "um  tres\n cinco");
        let mut text = "um dois\nsete oito".to_string();
        let mut carets = at(&[3, 8]);
        carets.delete_forward(&mut text, true);
        assert_eq!(text, "um \n oito");
    }

    #[test]
    fn two_carets_in_a_word_take_it_once() {
        let mut text = "ab cd".to_string();
        let mut carets = at(&[1, 2]);
        carets.backspace(&mut text, true);
        assert_eq!(text, " cd");
        assert_eq!(places(&carets), vec![0]);
    }

    #[test]
    fn up_and_down_keep_the_column_past_short_lines() {
        let text = "abcdef\nx\nabcdef";
        let mut carets = Carets::one((5, 5));
        carets.go(text, Move::Down, false);
        assert_eq!(places(&carets), vec![8]);
        carets.go(text, Move::Down, false);
        assert_eq!(places(&carets), vec![14]);
        carets.go(text, Move::Up, false);
        carets.go(text, Move::Up, false);
        assert_eq!(places(&carets), vec![5]);
    }

    #[test]
    fn carets_meeting_become_one() {
        let text = "ab\ncd";
        let mut carets = at(&[0, 3]);
        carets.go(text, Move::Up, false);
        assert_eq!(places(&carets), vec![0]);
        assert_eq!(carets.len(), 1);
    }

    #[test]
    fn line_keys_go_to_the_text_then_the_line() {
        let text = "  ab\n  cd";
        let mut carets = at(&[4, 9]);
        carets.go(text, Move::LineStart, false);
        assert_eq!(places(&carets), vec![2, 7]);
        carets.go(text, Move::LineStart, false);
        assert_eq!(places(&carets), vec![0, 5]);
        carets.go(text, Move::LineEnd, false);
        assert_eq!(places(&carets), vec![4, 9]);
    }

    #[test]
    fn an_arrow_goes_to_the_edge_of_a_selection() {
        let text = "abcdef";
        let mut carets = Carets::one((1, 3));
        carets.go(text, Move::Left, false);
        assert_eq!(places(&carets), vec![1]);
        carets.go(text, Move::Right, true);
        assert_eq!(carets.primary().sel(), (1, 2));
    }

    #[test]
    fn adding_lines_goes_outwards_and_stops_at_the_ends() {
        let text = "abcd\nab\nabcd";
        let mut carets = Carets::one((3, 3));
        carets.add_line(text, true);
        assert_eq!(carets.len(), 1);
        carets.add_line(text, false);
        carets.add_line(text, false);
        assert_eq!(places(&carets), vec![3, 7, 11]);
        assert_eq!(carets.primary_index(), 2);
        carets.add_line(text, false);
        assert_eq!(carets.len(), 3);
    }

    #[test]
    fn a_second_click_takes_a_caret_away_but_never_the_last() {
        let mut carets = at(&[1, 4]);
        carets.toggle(4);
        assert_eq!(places(&carets), vec![1]);
        carets.toggle(1);
        assert_eq!(places(&carets), vec![1]);
    }

    #[test]
    fn a_box_covers_the_same_columns_on_every_line() {
        let text = "abcdef\nxy\n\nabcdef";
        let carets = Carets::column(text, (0, 1), (3, 3));
        let sels: Vec<Sel> = carets.list().iter().map(|c| c.sel()).collect();
        // The empty line is too short to reach the box; the short one gives what it has.
        assert_eq!(sels, vec![(1, 3), (8, 9), (12, 14)]);
        let line = Carets::column(text, (0, 4), (2, 4));
        assert_eq!(places(&line), vec![4, 9, 10]);
    }

    #[test]
    fn a_box_on_a_huge_text_stops_at_the_limit() {
        let text = "x\n".repeat(MAX_CARETS * 2);
        let carets = Carets::column(&text, (0, 0), (MAX_CARETS * 2, 0));
        assert_eq!(carets.len(), MAX_CARETS);
    }

    #[test]
    fn pasting_as_many_lines_as_carets_spreads_them() {
        let mut text = "a\nb\n".to_string();
        let mut carets = at(&[1, 3]);
        carets.paste(&mut text, "1\n2");
        assert_eq!(text, "a1\nb2\n");
        let mut carets = at(&[2, 5]);
        carets.paste(&mut text, "z");
        assert_eq!(text, "a1z\nb2z\n");
    }

    #[test]
    fn copying_takes_the_selections_or_the_lines() {
        let mut text = "um\ndois\ntres".to_string();
        let mut carets = Carets::one((0, 2));
        carets.add(Caret::between(3, 7));
        assert_eq!(carets.copied(&text), "um\ndois");
        let mut empties = at(&[1, 2, 9]);
        assert_eq!(empties.copied(&text), "um\ntres\n");
        empties.cut(&mut text);
        assert_eq!(text, "dois\n");
    }
}
