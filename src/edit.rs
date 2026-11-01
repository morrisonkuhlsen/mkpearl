//! Source edits behind the ribbon. Pure functions over text and char offsets.

/// A selection as char offsets (start <= end).
pub type Sel = (usize, usize);

pub fn byte_of(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(b, _)| b)
}

pub fn char_of(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    text.char_indices().take_while(|(b, _)| *b < byte).count()
}

/// Wraps the selection in `pre`/`post`; with an empty selection the caret lands between them.
pub fn wrap(text: &mut String, (a, b): Sel, pre: &str, post: &str) -> Sel {
    let (ba, bb) = (byte_of(text, a), byte_of(text, b));
    text.insert_str(bb, post);
    text.insert_str(ba, pre);
    let n = pre.chars().count();
    (a + n, b + n)
}

/// Replaces the selection with `snippet`. A `|` in it marks where the caret goes (`\|` is literal
/// nowhere in our snippets, so the first `|` always is the marker).
pub fn insert(text: &mut String, (a, b): Sel, snippet: &str) -> Sel {
    let (before, after) = snippet.split_once('|').unwrap_or((snippet, ""));
    let (ba, bb) = (byte_of(text, a), byte_of(text, b));
    text.replace_range(ba..bb, &format!("{before}{after}"));
    let c = a + before.chars().count();
    (c, c)
}

fn line_bounds(text: &str, byte: usize) -> (usize, usize) {
    let start = text[..byte].rfind('\n').map_or(0, |i| i + 1);
    let end = text[byte..].find('\n').map_or(text.len(), |i| byte + i);
    (start, end)
}

/// The block marker a line starts with: heading `=`s, a list `-` or an enumeration `+`.
fn marker_len(line: &str) -> usize {
    let eq = line.chars().take_while(|c| *c == '=').count();
    if eq > 0 && line[eq..].starts_with(' ') {
        return eq + 1;
    }
    if line.starts_with("- ") || line.starts_with("+ ") || line.starts_with("/ ") {
        return 2;
    }
    0
}

/// Sets the caret line's block marker (`= `, `- `…), toggling it off when it already has that one.
pub fn line_marker(text: &mut String, (a, b): Sel, marker: &str) -> Sel {
    let (start, end) = line_bounds(text, byte_of(text, a));
    let line = &text[start..end];
    let old = marker_len(line);
    let same = &line[..old] == marker;
    let new = if same { "" } else { marker };
    let delta = new.chars().count() as isize - old as isize;
    text.replace_range(start..start + old, new);
    let start_c = char_of(text, start);
    let shift = |c: usize| (c as isize + delta).max(start_c as isize) as usize;
    (shift(a), shift(b))
}

/// Sets a document-wide rule: replaces the line starting with `key`, or adds `line` at the top.
pub fn set_rule(text: &mut String, (a, b): Sel, key: &str, line: &str) -> Sel {
    let mut offset = 0;
    for l in text.split_inclusive('\n') {
        if l.starts_with(key) {
            let old = l.trim_end_matches('\n');
            let delta = line.chars().count() as isize - old.chars().count() as isize;
            let old_start_c = char_of(text, offset);
            let old_end_c = old_start_c + old.chars().count();
            let (start, end) = (offset, offset + old.len());
            text.replace_range(start..end, line);
            let shift = |c: usize| if c >= old_end_c { (c as isize + delta) as usize } else { c.min(old_start_c + line.chars().count()) };
            return (shift(a), shift(b));
        }
        offset += l.len();
    }
    text.insert_str(0, &format!("{line}\n"));
    let n = line.chars().count() + 1;
    (a + n, b + n)
}

/// A table with `cols` columns and `rows` body rows; the caret lands in the first cell.
pub fn table_snippet(rows: usize, cols: usize, header: bool) -> String {
    let (rows, cols) = (rows.clamp(1, 100), cols.clamp(1, 26));
    let mut s = format!("#table(\n  columns: {cols},\n");
    if header {
        let cells: Vec<String> = (1..=cols).map(|i| format!("[*Coluna {i}*]")).collect();
        s += &format!("  table.header({}),\n", cells.join(", "));
    }
    let mut first = true;
    for _ in 0..rows {
        s += "  ";
        for c in 0..cols {
            s += if first { "[|]" } else { "[]" };
            first = false;
            s += if c + 1 == cols { ",\n" } else { ", " };
        }
    }
    s + ")\n"
}

/// Replaces the selection with `text` as it is; the caret lands after it.
pub fn insert_plain(buffer: &mut String, (a, b): Sel, text: &str) -> Sel {
    let (ba, bb) = (byte_of(buffer, a), byte_of(buffer, b));
    buffer.replace_range(ba..bb, text);
    let c = a + text.chars().count();
    (c, c)
}

/// What the Files pane inserts for a file of the project, by its path from the document's folder.
pub fn file_snippet(path: &str) -> String {
    let quoted = path.replace('\\', "\\\\").replace('"', "\\\"");
    let extension = path.rsplit('.').next().unwrap_or_default().to_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => format!("#figure(\n  image(\"{quoted}\"),\n  caption: [Legenda],\n)\n"),
        "typ" => format!("#include \"{quoted}\"\n"),
        "bib" => format!("#bibliography(\"{quoted}\")\n"),
        _ => format!("\"{quoted}\""),
    }
}

/// How well `label` fits what's been typed, ignoring case: 0 the same, 1 starts with it, 2 contains
/// it, 3 has its letters in order; `None` when it doesn't fit.
pub fn match_rank(query: &str, label: &str) -> Option<u8> {
    let (query, label) = (query.to_lowercase(), label.to_lowercase());
    if query == label {
        return Some(0);
    }
    if label.starts_with(&query) {
        return Some(1);
    }
    if label.contains(&query) {
        return Some(2);
    }
    let mut letters = label.chars();
    query.chars().all(|q| letters.any(|l| l == q)).then_some(3)
}

/// What closes the brackets still open at `byte`, innermost first. Strings and comments don't count.
pub fn open_brackets(text: &str, byte: usize) -> String {
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut chars = text.get(..byte).unwrap_or(text).chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '"' => in_string = !in_string,
            _ if in_string => {}
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            ')' | ']' | '}' if stack.last() == Some(&c) => {
                stack.pop();
            }
            _ => {}
        }
    }
    let closers: String = stack.iter().rev().collect();
    if in_string { format!("\"{closers}") } else { closers }
}

/// Typst's documentation as plain words: links keep their text, and the marks go.
pub fn plain_doc(doc: &str) -> String {
    let mut out = String::with_capacity(doc.len());
    let mut rest = doc;
    while let Some(open) = rest.find('[') {
        let (before, after) = rest.split_at(open);
        out.push_str(before);
        // `[text](target)` becomes `text`; any other bracket stays as it is.
        match after.find("](") {
            Some(mid) if !after.get(1..mid).unwrap_or_default().contains(['[', '\n']) => {
                let tail = after.get(mid + 2..).unwrap_or_default();
                out.push_str(after.get(1..mid).unwrap_or_default());
                rest = tail.find(')').map_or("", |end| tail.get(end + 1..).unwrap_or_default());
            }
            _ => {
                out.push('[');
                rest = after.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "")
}

/// A completion's text without its `${…}` placeholders, and where the first placeholder is, as a
/// char range of the plain text (the caret, or what's to be typed over).
pub fn expand_snippet(snippet: &str) -> (String, Option<Sel>) {
    let mut plain = String::with_capacity(snippet.len());
    let mut first = None;
    let mut rest = snippet;
    while let Some((before, after)) = rest.split_once("${") {
        let Some((name, tail)) = after.split_once('}') else { break };
        plain.push_str(before);
        let start = plain.chars().count();
        plain.push_str(name);
        first.get_or_insert((start, start + name.chars().count()));
        rest = tail;
    }
    plain.push_str(rest);
    (plain, first)
}

/// Replaces the bytes `from..to` with a completion; the selection lands on its first placeholder,
/// or after it when it has none.
pub fn apply_completion(text: &mut String, from: usize, to: usize, snippet: &str) -> Sel {
    let (from, to) = (from.min(text.len()), to.min(text.len()));
    let (from, to) = (from.min(to), to.max(from));
    let (plain, first) = expand_snippet(snippet);
    let Some(_) = text.get(from..to) else { return (char_of(text, to), char_of(text, to)) };
    let start = char_of(text, from);
    text.replace_range(from..to, &plain);
    match first {
        Some((a, b)) => (start + a, start + b),
        None => {
            let end = start + plain.chars().count();
            (end, end)
        }
    }
}

/// The selected text.
pub fn selected(text: &str, (a, b): Sel) -> &str {
    text.get(byte_of(text, a)..byte_of(text, b)).unwrap_or_default()
}

/// Removes the selection; the caret stays where it began.
pub fn delete(text: &mut String, (a, b): Sel) -> Sel {
    let (ba, bb) = (byte_of(text, a), byte_of(text, b));
    text.replace_range(ba..bb, "");
    (a, a)
}

/// Comments out the selected lines with `// `, or takes it away when they all have it.
pub fn toggle_comment(text: &mut String, (a, b): Sel) -> Sel {
    let (start, _) = line_bounds(text, byte_of(text, a));
    let (_, end) = line_bounds(text, byte_of(text, b));
    let block = &text[start..end];
    let off = block.lines().filter(|l| !l.trim().is_empty()).all(|l| l.starts_with("//")) && !block.trim().is_empty();
    let changed: Vec<String> = block
        .split('\n')
        .map(|l| {
            if off {
                l.strip_prefix("// ").or_else(|| l.strip_prefix("//")).unwrap_or(l).to_string()
            } else if l.trim().is_empty() {
                l.to_string()
            } else {
                format!("// {l}")
            }
        })
        .collect();
    let changed = changed.join("\n");
    let start_c = char_of(text, start);
    let n = changed.chars().count();
    text.replace_range(start..end, &changed);
    (start_c, start_c + n)
}

/// Every match of `query`, ignoring case, not overlapping, as char ranges, up to `limit`.
pub fn find_all(text: &str, query: &str, limit: usize) -> Vec<Sel> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at + needle.len() <= hay.len() && out.len() < limit {
        let fits = hay.get(at..at + needle.len()).is_some_and(|w| w.iter().zip(&needle).all(|(h, n)| h.to_lowercase().eq(std::iter::once(*n))));
        if fits {
            out.push((at, at + needle.len()));
            at += needle.len();
        } else {
            at += 1;
        }
    }
    out
}

/// The next match of `query` from char `from`, ignoring case, wrapping round the end of the text.
pub fn find(text: &str, query: &str, from: usize, backwards: bool) -> Option<Sel> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let hay: Vec<char> = text.chars().collect();
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let same = |at: usize| hay.get(at..at + needle.len()).is_some_and(|w| w.iter().zip(&needle).all(|(h, n)| h.to_lowercase().eq(std::iter::once(*n))));
    let last = hay.len() - needle.len();
    let from = from.min(last + 1);
    let found = if backwards {
        (0..from).rev().find(|i| same(*i)).or_else(|| (from..=last).rev().find(|i| same(*i)))
    } else {
        (from..=last).find(|i| same(*i)).or_else(|| (0..from).find(|i| same(*i)))
    };
    found.map(|i| (i, i + needle.len()))
}

/// The 1-based line of a byte offset.
pub fn line_of(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte.min(text.len())].iter().filter(|b| **b == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_keeps_the_selection_on_the_text() {
        let mut t = "olá mundo".to_string();
        assert_eq!(wrap(&mut t, (4, 9), "*", "*"), (5, 10));
        assert_eq!(t, "olá *mundo*");
    }

    #[test]
    fn wrap_with_no_selection_puts_the_caret_inside() {
        let mut t = "é".to_string();
        assert_eq!(wrap(&mut t, (1, 1), "#sub[", "]"), (6, 6));
        assert_eq!(t, "é#sub[]");
    }

    #[test]
    fn insert_places_the_caret_at_the_marker() {
        let mut t = "ab".to_string();
        assert_eq!(insert(&mut t, (1, 1), "$ | $"), (3, 3));
        assert_eq!(t, "a$  $b");
    }

    #[test]
    fn insert_replaces_the_selection() {
        let mut t = "ação".to_string();
        insert(&mut t, (1, 3), "X");
        assert_eq!(t, "aXo");
    }

    #[test]
    fn line_marker_sets_replaces_and_toggles() {
        let mut t = "um\ndois".to_string();
        let s = line_marker(&mut t, (5, 5), "= ");
        assert_eq!((t.as_str(), s), ("um\n= dois", (7, 7)));
        let s = line_marker(&mut t, s, "== ");
        assert_eq!((t.as_str(), s), ("um\n== dois", (8, 8)));
        let s = line_marker(&mut t, s, "== ");
        assert_eq!((t.as_str(), s), ("um\ndois", (5, 5)));
    }

    #[test]
    fn set_rule_adds_then_replaces() {
        let mut t = "ç\n".to_string();
        let s = set_rule(&mut t, (1, 1), "#set page(paper", "#set page(paper: \"a4\")");
        assert_eq!(t, "#set page(paper: \"a4\")\nç\n");
        assert_eq!(s, (24, 24));
        let s = set_rule(&mut t, s, "#set page(paper", "#set page(paper: \"a5\")");
        assert_eq!(t, "#set page(paper: \"a5\")\nç\n");
        assert_eq!(s, (24, 24));
    }

    #[test]
    fn file_snippets_fit_the_kind_of_file_and_escape_the_name() {
        assert!(file_snippet("fig/a b.PNG").contains("image(\"fig/a b.PNG\")"));
        assert_eq!(file_snippet("cap.typ"), "#include \"cap.typ\"\n");
        assert_eq!(file_snippet("refs.bib"), "#bibliography(\"refs.bib\")\n");
        assert_eq!(file_snippet("a\"b|c.csv"), "\"a\\\"b|c.csv\"");
    }

    #[test]
    fn plain_insert_keeps_every_character() {
        let mut t = "ab".to_string();
        assert_eq!(insert_plain(&mut t, (1, 1), "x|y"), (4, 4));
        assert_eq!(t, "ax|yb");
    }

    #[test]
    fn snippets_lose_their_placeholders_and_mark_the_first() {
        assert_eq!(expand_snippet("align"), ("align".to_string(), None));
        assert_eq!(expand_snippet("align(${})"), ("align()".to_string(), Some((6, 6))));
        assert_eq!(expand_snippet("*${strong}*"), ("*strong*".to_string(), Some((1, 7))));
        assert_eq!(expand_snippet("\"${text}\": ${}"), ("\"text\": ".to_string(), Some((1, 5))));
        assert_eq!(expand_snippet("ação ${é} ${}"), ("ação é ".to_string(), Some((5, 6))));
        // An unclosed placeholder is plain text, not a panic.
        assert_eq!(expand_snippet("a${b"), ("a${b".to_string(), None));
    }

    #[test]
    fn a_completion_replaces_what_was_typed() {
        let mut t = "é #ali fim".to_string();
        let from = "é #".len();
        let sel = apply_completion(&mut t, from, from + 3, "align(${})");
        assert_eq!(t, "é #align() fim");
        assert_eq!(sel, (9, 9));
    }

    #[test]
    fn hostile_completion_ranges_do_not_panic() {
        let mut t = "ação".to_string();
        apply_completion(&mut t, 2, 2, "x"); // inside the ç
        assert_eq!(t, "ação");
        apply_completion(&mut t, 99, 999, "y");
        assert_eq!(t, "açãoy");
    }

    #[test]
    fn matches_rank_from_exact_to_scattered() {
        assert_eq!(match_rank("align", "align"), Some(0));
        assert_eq!(match_rank("ALI", "align"), Some(1));
        assert_eq!(match_rank("lig", "align"), Some(2));
        assert_eq!(match_rank("agn", "align"), Some(3));
        assert_eq!(match_rank("xyz", "align"), None);
        assert_eq!(match_rank("", "align"), Some(1));
    }

    #[test]
    fn open_brackets_are_found_innermost_first() {
        assert_eq!(open_brackets("#set text(", 10), ")");
        assert_eq!(open_brackets("#f(a, [b", 8), "])");
        assert_eq!(open_brackets("#f(a) ok", 8), "");
        assert_eq!(open_brackets("#f(\"(\", ", 8), ")");
        assert_eq!(open_brackets("#f(\"ab", 6), "\")");
        assert_eq!(open_brackets("// (\n#g(", 8), ")");
        assert_eq!(open_brackets("sem par )", 9), "");
        assert_eq!(open_brackets("ação", 999), "");
    }

    #[test]
    fn documentation_reads_as_plain_words() {
        assert_eq!(plain_doc("Aligns [content]($align) `here` **now**."), "Aligns content here now.");
        assert_eq!(plain_doc("a [b] c"), "a [b] c");
        assert_eq!(plain_doc("[x](y"), "x");
        assert_eq!(plain_doc("[" ), "[");
        assert_eq!(plain_doc("ação [é](ü) ç"), "ação é ç");
    }

    #[test]
    fn every_match_is_found_without_overlap_and_within_the_limit() {
        assert_eq!(find_all("Ação ação", "AÇÃO", 10), [(0, 4), (5, 9)]);
        assert_eq!(find_all("aaaa", "aa", 10), [(0, 2), (2, 4)]);
        assert_eq!(find_all("aaaa", "a", 2).len(), 2);
        assert!(find_all("abc", "", 10).is_empty());
        assert!(find_all("a", "abc", 10).is_empty());
    }

    #[test]
    fn a_term_list_marker_counts_as_a_marker() {
        let mut t = "/ termo".to_string();
        line_marker(&mut t, (3, 3), "- ");
        assert_eq!(t, "- termo");
    }

    #[test]
    fn delete_and_selected_work_on_chars() {
        let mut t = "ação feita".to_string();
        assert_eq!(selected(&t, (1, 4)), "ção");
        assert_eq!(delete(&mut t, (1, 4)), (1, 1));
        assert_eq!(t, "a feita");
    }

    #[test]
    fn comments_toggle_over_whole_lines_and_skip_blank_ones() {
        let mut t = "ação\n\nfim\nresto".to_string();
        let s = toggle_comment(&mut t, (2, 7));
        assert_eq!(t, "// ação\n\n// fim\nresto");
        toggle_comment(&mut t, s);
        assert_eq!(t, "ação\n\nfim\nresto");
    }

    #[test]
    fn find_ignores_case_wraps_and_goes_back() {
        let t = "Ação ação AÇÃO";
        assert_eq!(find(t, "ação", 0, false), Some((0, 4)));
        assert_eq!(find(t, "ação", 1, false), Some((5, 9)));
        assert_eq!(find(t, "ação", 11, false), Some((0, 4)));
        assert_eq!(find(t, "ação", 5, true), Some((0, 4)));
        assert_eq!(find(t, "ação", 0, true), Some((10, 14)));
        assert_eq!(find(t, "", 0, false), None);
        assert_eq!(find("a", "abc", 0, false), None);
    }

    #[test]
    fn hostile_table_sizes_are_clamped() {
        assert!(table_snippet(0, 0, false).contains("columns: 1"));
        assert!(table_snippet(9, 9999, true).contains("columns: 26"));
    }
}
