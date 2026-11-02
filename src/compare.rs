//! What changed between two versions of a source, word by word, ignoring spaces: what left, what came,
//! and what only changed place. The pages show these; see `Compiler::compare`.

use std::ops::Range;

/// Past this, the common subsequence isn't worked out and a whole stretch counts as changed.
const MAX_CELLS: usize = 4_000_000;
/// A moved stretch has at least this many bytes: shorter ones, such as "de", repeat by chance.
const LEAST_MOVED: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Removed,
    Moved,
}

/// The changed stretches of each version, in order, not overlapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub old: Vec<(Range<usize>, Kind)>,
    pub new: Vec<(Range<usize>, Kind)>,
}

/// The kind of what's at this byte, among stretches in order.
pub fn kind_at(stretches: &[(Range<usize>, Kind)], byte: usize) -> Option<usize> {
    let after = stretches.partition_point(|(range, _)| range.start <= byte);
    let index = after.checked_sub(1)?;
    stretches.get(index).filter(|(range, _)| byte < range.end).map(|_| index)
}

/// Words, and each other mark on its own. Spaces divide them and count for nothing.
fn tokens(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices() {
        if c.is_alphanumeric() || c == '_' {
            start.get_or_insert(at);
            continue;
        }
        if let Some(from) = start.take() {
            out.push(from..at);
        }
        if !c.is_whitespace() {
            out.push(at..at + c.len_utf8());
        }
    }
    if let Some(from) = start {
        out.push(from..text.len());
    }
    out
}

/// The pairs of a longest common subsequence, or none when it would take too long to work out.
fn common<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 || n.saturating_mul(m) > MAX_CELLS {
        return Vec::new();
    }
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    let cell = |table: &[u32], i: usize, j: usize| table.get(i * width + j).copied().unwrap_or(0);
    for (i, x) in a.iter().enumerate().rev() {
        for (j, y) in b.iter().enumerate().rev() {
            let value = if x == y { cell(&table, i + 1, j + 1) + 1 } else { cell(&table, i + 1, j).max(cell(&table, i, j + 1)) };
            if let Some(slot) = table.get_mut(i * width + j) {
                *slot = value;
            }
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut pairs = Vec::new();
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        if x == y {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if cell(&table, i + 1, j) >= cell(&table, i, j + 1) {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

/// Marks which tokens of each side have a partner, working on what's left after the common ends.
fn match_tokens<'a>(old: &'a [&'a str], new: &'a [&'a str], old_kept: &mut [bool], new_kept: &mut [bool]) {
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let rest = |list: &'_ [&str]| list.len().saturating_sub(start);
    let end = old.iter().rev().take(rest(old)).zip(new.iter().rev().take(rest(new))).take_while(|(a, b)| a == b).count();
    let keep = |kept: &mut [bool], at: usize| {
        if let Some(slot) = kept.get_mut(at) {
            *slot = true;
        }
    };
    for i in 0..start {
        keep(old_kept, i);
        keep(new_kept, i);
    }
    for i in 1..=end {
        keep(old_kept, old.len() - i);
        keep(new_kept, new.len() - i);
    }
    let middle = |list: &'a [&'a str]| list.get(start..list.len() - end).unwrap_or_default();
    for (i, j) in common(middle(old), middle(new)) {
        keep(old_kept, start + i);
        keep(new_kept, start + j);
    }
}

fn stretches(tokens: &[Range<usize>], kept: &[bool], kind: Kind) -> Vec<(Range<usize>, Kind)> {
    let mut out: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut joining = false;
    for (token, kept) in tokens.iter().zip(kept) {
        if *kept {
            joining = false;
        } else if joining && let Some((last, _)) = out.last_mut() {
            last.end = token.end;
        } else {
            out.push((token.clone(), kind));
            joining = true;
        }
    }
    out
}

fn without_spaces(text: &str) -> String {
    text.split_whitespace().collect()
}

/// The lines with text in them, trimmed, and where each is.
fn lines(text: &str) -> Vec<(Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let start = at + (line.len() - line.trim_start().len());
            out.push((start..start + trimmed.len(), trimmed));
        }
        at += line.len();
    }
    out
}

/// Lines that stay, in both versions, in order; and whole lines found in another place in the other
/// version, which a word by word look would take for changes.
struct LineMatch {
    stays: Vec<(Range<usize>, Range<usize>)>,
    old_moved: Vec<Range<usize>>,
    new_moved: Vec<Range<usize>>,
}

fn match_lines(old: &str, new: &str) -> LineMatch {
    let (old_lines, new_lines) = (lines(old), lines(new));
    let old_texts: Vec<&str> = old_lines.iter().map(|(_, text)| *text).collect();
    let new_texts: Vec<&str> = new_lines.iter().map(|(_, text)| *text).collect();
    let pairs = common(&old_texts, &new_texts);
    let mut old_free = vec![true; old_lines.len()];
    let mut new_free = vec![true; new_lines.len()];
    let mut stays = Vec::with_capacity(pairs.len());
    for (i, j) in pairs {
        if let (Some(a), Some(b), Some((x, _)), Some((y, _))) = (old_free.get_mut(i), new_free.get_mut(j), old_lines.get(i), new_lines.get(j)) {
            *a = false;
            *b = false;
            stays.push((x.clone(), y.clone()));
        }
    }
    let (mut old_moved, mut new_moved) = (Vec::new(), Vec::new());
    for ((range, text), free) in old_lines.iter().zip(&old_free) {
        if !free || range.len() < LEAST_MOVED {
            continue;
        }
        let partner = new_lines.iter().zip(new_free.iter_mut()).find(|((_, other), other_free)| **other_free && other == text);
        if let Some(((other, _), other_free)) = partner {
            *other_free = false;
            old_moved.push(range.clone());
            new_moved.push(other.clone());
        }
    }
    LineMatch { stays, old_moved, new_moved }
}

/// Marks the tokens that have a partner: everything on a line that stays, and, between those lines,
/// what the words have in common there and only there.
fn match_around_lines(
    old: &str,
    new: &str,
    old_tokens: &[Range<usize>],
    new_tokens: &[Range<usize>],
    stays: &[(Range<usize>, Range<usize>)],
) -> (Vec<bool>, Vec<bool>) {
    let (mut old_kept, mut new_kept) = (vec![false; old_tokens.len()], vec![false; new_tokens.len()]);
    let (mut old_from, mut new_from) = (0, 0);
    let ends = stays.iter().map(|(a, b)| (a.clone(), b.clone())).chain(std::iter::once((old.len()..old.len(), new.len()..new.len())));
    for (old_line, new_line) in ends {
        let within = |tokens: &[Range<usize>], from: usize, to: usize| -> Vec<usize> {
            tokens.iter().enumerate().filter(|(_, t)| t.start >= from && t.start < to).map(|(i, _)| i).collect()
        };
        let (old_gap, new_gap) = (within(old_tokens, old_from, old_line.start), within(new_tokens, new_from, new_line.start));
        let words = |text: &'_ str, tokens: &[Range<usize>], gap: &[usize]| -> Vec<String> {
            gap.iter().filter_map(|i| tokens.get(*i)).map(|t| text.get(t.clone()).unwrap_or_default().to_string()).collect()
        };
        let (old_words, new_words) = (words(old, old_tokens, &old_gap), words(new, new_tokens, &new_gap));
        let old_refs: Vec<&str> = old_words.iter().map(String::as_str).collect();
        let new_refs: Vec<&str> = new_words.iter().map(String::as_str).collect();
        let (mut a, mut b) = (vec![false; old_refs.len()], vec![false; new_refs.len()]);
        match_tokens(&old_refs, &new_refs, &mut a, &mut b);
        for (kept, index) in a.iter().zip(&old_gap) {
            if let Some(slot) = old_kept.get_mut(*index) {
                *slot = *kept;
            }
        }
        for (kept, index) in b.iter().zip(&new_gap) {
            if let Some(slot) = new_kept.get_mut(*index) {
                *slot = *kept;
            }
        }
        for (kept, tokens, line) in [(&mut old_kept, old_tokens, &old_line), (&mut new_kept, new_tokens, &new_line)] {
            for (slot, token) in kept.iter_mut().zip(tokens) {
                if line.contains(&token.start) {
                    *slot = true;
                }
            }
        }
        (old_from, new_from) = (old_line.end, new_line.end);
    }
    (old_kept, new_kept)
}

/// What changed from `old` to `new`.
pub fn changes(old: &str, new: &str) -> Changes {
    let LineMatch { stays, old_moved, new_moved } = match_lines(old, new);
    let outside = |moved: &[Range<usize>], token: &Range<usize>| !moved.iter().any(|range| range.contains(&token.start));
    let old_tokens: Vec<Range<usize>> = tokens(old).into_iter().filter(|t| outside(&old_moved, t)).collect();
    let new_tokens: Vec<Range<usize>> = tokens(new).into_iter().filter(|t| outside(&new_moved, t)).collect();
    let (old_kept, new_kept) = match_around_lines(old, new, &old_tokens, &new_tokens, &stays);
    let mut found = Changes { old: stretches(&old_tokens, &old_kept, Kind::Removed), new: stretches(&new_tokens, &new_kept, Kind::Added) };

    // What left in one place and came in another, unchanged, only moved.
    for (range, kind) in &mut found.old {
        if range.len() < LEAST_MOVED {
            continue;
        }
        let text = without_spaces(old.get(range.clone()).unwrap_or_default());
        let partner = found
            .new
            .iter_mut()
            .find(|(other, other_kind)| *other_kind == Kind::Added && without_spaces(new.get(other.clone()).unwrap_or_default()) == text);
        if let Some((_, other_kind)) = partner {
            *kind = Kind::Moved;
            *other_kind = Kind::Moved;
        }
    }
    found.old.extend(old_moved.into_iter().map(|range| (range, Kind::Moved)));
    found.new.extend(new_moved.into_iter().map(|range| (range, Kind::Moved)));
    found.old.sort_by_key(|(range, _)| range.start);
    found.new.sort_by_key(|(range, _)| range.start);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen<'a>(text: &'a str, stretches: &[(Range<usize>, Kind)]) -> Vec<(&'a str, Kind)> {
        stretches.iter().map(|(range, kind)| (&text[range.clone()], *kind)).collect()
    }

    #[test]
    fn words_that_came_and_left_are_found() {
        let (old, new) = ("O gato preto dorme.", "O gato branco dorme muito.");
        let found = changes(old, new);
        assert_eq!(seen(old, &found.old), [("preto", Kind::Removed)]);
        assert_eq!(seen(new, &found.new), [("branco", Kind::Added), ("muito", Kind::Added)]);
    }

    #[test]
    fn a_stretch_is_one_change_and_spaces_are_none() {
        let (old, new) = ("Um dois", "Um   três quatro cinco\ndois");
        assert_eq!(seen(new, &changes(old, new).new), [("três quatro cinco", Kind::Added)]);
        assert_eq!(changes("a  b\nc", "a b c"), Changes::default());
        assert_eq!(changes("", ""), Changes::default());
    }

    #[test]
    fn what_changed_place_is_told_apart_from_what_changed() {
        let moved = "Um parágrafo que mudou de lugar.";
        let (old, new) = (format!("{moved}\n\nFica aqui.\n"), format!("Fica aqui.\n\n{moved}\n"));
        let found = changes(&old, &new);
        assert_eq!(seen(&old, &found.old), [(moved, Kind::Moved)]);
        assert_eq!(seen(&new, &found.new), [(moved, Kind::Moved)]);
        // Short words repeat by chance.
        let found = changes("de um lado fica", "um lado de fica");
        assert!(found.new.iter().chain(&found.old).all(|(_, kind)| *kind != Kind::Moved), "{found:?}");
    }

    #[test]
    fn a_line_that_stays_keeps_its_marks_to_itself() {
        // The period of the line that stayed isn't paired with one in another paragraph.
        let (old, new) = ("Fica no lugar.\n\nOutra coisa.\n", "Outra coisa nova.\n\nFica no lugar.\n");
        let found = changes(old, new);
        assert!(found.old.iter().all(|(range, _)| !old[range.clone()].contains("lugar")), "{found:?}");
        assert!(found.old.iter().all(|(range, _)| &old[range.clone()] != "."), "{found:?}");
    }

    #[test]
    fn a_huge_change_is_all_changed_and_not_slow() {
        let old: String = (0..6000).map(|i| format!("a{i} ")).collect();
        let new: String = (0..6000).map(|i| format!("b{i} ")).collect();
        let found = changes(&old, &new);
        assert_eq!(found.old.len(), 1);
        assert_eq!(found.new.len(), 1);
    }

    #[test]
    fn a_byte_finds_its_stretch() {
        let stretches = [(2..4, Kind::Added), (6..7, Kind::Removed)];
        assert_eq!(kind_at(&stretches, 2), Some(0));
        assert_eq!(kind_at(&stretches, 3), Some(0));
        assert_eq!(kind_at(&stretches, 4), None);
        assert_eq!(kind_at(&stretches, 6), Some(1));
        assert_eq!(kind_at(&stretches, 0), None);
        assert_eq!(kind_at(&[], 0), None);
    }

    #[test]
    fn accents_cut_nothing() {
        let (old, new) = ("ação é ótima", "ação não é ótima");
        assert_eq!(seen(new, &changes(old, new).new), [("não", Kind::Added)]);
    }
}
