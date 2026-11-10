//! Source edits behind the ribbon. Pure functions over text and char offsets.

use std::ops::Range;

use typst::syntax::{LinkedNode, Side, SyntaxKind, parse};

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

/// Sets the page's margins: replaces the value in the document's last `#set page(margin: …)`, whatever
/// else that rule says and however it's written, or adds a rule at the top when there is none.
pub fn set_page_margin(text: &mut String, sel: Sel, value: &str) -> Sel {
    let found = last_set_value(text, "page", "margin", true);
    write_set_value(text, sel, found, "page", "margin", value)
}

/// Sets an argument for the whole document: replaces the value in its last rule that gives it, whatever
/// else that rule says, or adds a rule at the top when none does. A rule inside a passage only speaks
/// for that passage, and stays as it is.
pub fn set_argument(text: &mut String, sel: Sel, function: &str, argument: &str, value: &str) -> Sel {
    let found = last_set_value(text, function, argument, false);
    write_set_value(text, sel, found, function, argument, value)
}

/// The font the document's rules give its text, the first one where they give a list.
pub fn document_font(source: &str) -> Option<String> {
    let value = source.get(last_set_value(source, "text", "font", false)?)?;
    value.split('"').nth(1).map(str::to_string)
}

fn write_set_value(text: &mut String, sel: Sel, found: Option<Range<usize>>, function: &str, argument: &str, value: &str) -> Sel {
    let change = match found {
        Some(range) => (range, value.to_string()),
        None => (0..0, format!("#set {function}({argument}: {value})\n")),
    };
    replace_keeping(text, sel, vec![change])
}

fn last_set_value(source: &str, function: &str, argument: &str, nested: bool) -> Option<Range<usize>> {
    let root = parse(source);
    let mut pending = vec![LinkedNode::new(&root)];
    let mut found: Option<Range<usize>> = None;
    while let Some(node) = pending.pop() {
        let reaches = nested || node.parent().is_some_and(|parent| parent.parent().is_none());
        if reaches
            && node.kind() == SyntaxKind::SetRule
            && node.children().any(|c| c.kind() == SyntaxKind::Ident && source.get(c.range()) == Some(function))
        {
            let args = node.children().find(|c| c.kind() == SyntaxKind::Args);
            for arg in args.iter().flat_map(LinkedNode::children).filter(|a| a.kind() == SyntaxKind::Named) {
                let mut parts = arg.children();
                if parts.next().is_some_and(|name| source.get(name.range()) == Some(argument))
                    && let Some(value) = arg.children().rfind(|p| !p.kind().is_trivia() && p.kind() != SyntaxKind::Colon)
                    && found.as_ref().is_none_or(|f| value.offset() > f.start)
                {
                    found = Some(value.range());
                }
            }
        }
        pending.extend(node.children());
    }
    found
}

/// An image call in the source, and the block on its own that holds it, where there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageCall {
    pub call: Range<usize>,
    /// From the `#` to the end of the call, with a label after it on the same line.
    pub block: Option<Range<usize>>,
}

/// The `image(...)` call at this byte, and what surrounds it.
pub fn image_call_at(source: &str, byte: usize) -> Option<ImageCall> {
    let root = parse(source);
    let top = LinkedNode::new(&root);
    let mut node = top.leaf_at(byte, Side::After)?;
    let call = loop {
        if node.kind() == SyntaxKind::FuncCall && node.children().next().is_some_and(|callee| source.get(callee.range()) == Some("image")) {
            break node.range();
        }
        node = node.parent()?.clone();
    };
    while let Some(parent) = node.parent().filter(|parent| parent.parent().is_some()) {
        node = parent.clone();
    }
    let block = (node.kind() == SyntaxKind::FuncCall).then_some(()).and_then(|()| {
        let hash = node.prev_sibling().filter(|sibling| sibling.kind() == SyntaxKind::Hash)?;
        let label = node.next_sibling().filter(|sibling| {
            sibling.kind() == SyntaxKind::Label && source.get(node.range().end..sibling.range().start).is_some_and(|between| !between.contains('\n'))
        });
        Some(hash.offset()..label.map_or(node.range().end, |l| l.range().end))
    });
    Some(ImageCall { call, block })
}

/// Replaces parts of the text, keeping the selection on the same letters.
pub(crate) fn replace_keeping(text: &mut String, sel: Sel, mut changes: Vec<(Range<usize>, String)>) -> Sel {
    changes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let (mut a, mut b) = sel;
    for (range, new) in changes {
        let (start, end) = (char_of(text, range.start), char_of(text, range.end));
        let new_end = start + new.chars().count();
        let shift = |c: usize| if c >= end { c - end + new_end } else { c.min(if c > start { new_end } else { c }) };
        (a, b) = (shift(a), shift(b));
        text.replace_range(range, &new);
    }
    (a, b)
}

/// Sets `width:` on the image call starting at this byte, adding it if it's not there. A `height:` goes:
/// the two together would no longer keep the picture's shape.
pub fn set_image_width(text: &mut String, sel: Sel, byte: usize, value: &str) -> Sel {
    let Some(found) = image_call_at(text, byte) else { return sel };
    let source = text.clone();
    let root = parse(&source);
    let top = LinkedNode::new(&root);
    let Some(mut node) = top.leaf_at(found.call.start, Side::After) else { return sel };
    while !(node.kind() == SyntaxKind::FuncCall && node.range() == found.call) {
        let Some(parent) = node.parent() else { return sel };
        node = parent.clone();
    }
    let Some(args) = node.children().find(|child| child.kind() == SyntaxKind::Args) else { return sel };
    let named = |name: &str| {
        args.children()
            .filter(|arg| arg.kind() == SyntaxKind::Named)
            .find(|arg| arg.children().next().is_some_and(|n| source.get(n.range()) == Some(name)))
    };
    let mut changes = Vec::new();
    if let Some(width) = named("width") {
        if let Some(old) = width.children().rfind(|p| !p.kind().is_trivia() && p.kind() != SyntaxKind::Colon) {
            changes.push((old.range(), value.to_string()));
        }
    } else if let Some(close) = args.children().rfind(|c| c.kind() == SyntaxKind::RightParen) {
        let last = args.children().rfind(|c| !c.kind().is_trivia() && c.kind() != SyntaxKind::RightParen);
        let comma = last.as_ref().is_some_and(|l| !matches!(l.kind(), SyntaxKind::LeftParen | SyntaxKind::Comma));
        let at = last.map_or(close.offset(), |l| l.range().end);
        changes.push((at..at, format!("{}width: {value}", if comma { ", " } else { " " })));
    }
    if let Some(height) = named("height") {
        // With the comma after it, or else the one before.
        let after = height.next_sibling().filter(|n| n.kind() == SyntaxKind::Comma).map(|n| n.range().end);
        let before = height.prev_sibling().filter(|n| n.kind() == SyntaxKind::Comma).map(|n| n.range().start);
        let mut range = height.range();
        if let Some(end) = after {
            range.end = end;
        } else if let Some(start) = before {
            range.start = start;
        }
        changes.push((range, String::new()));
    }
    replace_keeping(text, sel, changes)
}

/// Where a block goes when dropped by the text at `target`: on its own line, before or after that
/// paragraph, with whole code blocks and headings kept apart from it.
fn drop_line(source: &str, target: usize, above: bool) -> usize {
    let root = parse(source);
    let top = LinkedNode::new(&root);
    let node = top.children().find(|child| child.range().contains(&target));
    let (start, end) = node.map_or((target, target), |n| (n.range().start, n.range().end));
    let is_boundary = |line: &str| line.trim().is_empty() || line.starts_with(['#', '=']);
    if above {
        let mut at = source.get(..start).and_then(|s| s.rfind('\n')).map_or(0, |n| n + 1);
        while at > 0 {
            let previous_start = source.get(..at - 1).and_then(|s| s.rfind('\n')).map_or(0, |n| n + 1);
            if source.get(previous_start..at - 1).is_none_or(is_boundary) {
                break;
            }
            at = previous_start;
        }
        at
    } else {
        let mut at = source.get(end..).and_then(|s| s.find('\n')).map_or(source.len(), |n| end + n + 1);
        while at < source.len() {
            let next_end = source.get(at..).and_then(|s| s.find('\n')).map_or(source.len(), |n| at + n);
            if source.get(at..next_end).is_none_or(is_boundary) {
                break;
            }
            at = (next_end + 1).min(source.len());
        }
        at
    }
}

/// Moves a block of the source to where it's dropped. `None` when it would stay where it is. Returns
/// where the block's text now starts, in chars.
pub fn move_block(text: &mut String, block: Range<usize>, target: usize, above: bool) -> Option<usize> {
    let moved = text.get(block.clone())?.to_string();
    let whole_end = if text.get(block.end..)?.starts_with('\n') { block.end + 1 } else { block.end };
    let line = drop_line(text, target, above);
    if (block.start..=whole_end).contains(&line) {
        return None;
    }
    let at = if line > block.start { line - (whole_end - block.start) } else { line };
    text.replace_range(block.start..whole_end, "");
    let needs_line = at > 0 && text.get(..at).is_some_and(|before| !before.ends_with('\n'));
    let inserted = format!("{}{moved}\n", if needs_line { "\n" } else { "" });
    text.insert_str(at, &inserted);
    Some(char_of(text, at + usize::from(needs_line)))
}

/// Takes a block out of the source, with the line it stood on. Returns where it was, in chars.
pub fn delete_block(text: &mut String, block: Range<usize>) -> usize {
    let end = if text.get(block.end..).is_some_and(|rest| rest.starts_with('\n')) { block.end + 1 } else { block.end };
    let at = char_of(text, block.start);
    text.replace_range(block.start..end, "");
    at
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

/// Inserts a block of its own: starting on a new line unless the caret is already at the start of one.
pub fn insert_block(buffer: &mut String, (a, b): Sel, text: &str) -> Sel {
    let at_start = buffer.get(..byte_of(buffer, a)).is_none_or(|before| before.is_empty() || before.ends_with('\n'));
    if at_start { insert_plain(buffer, (a, b), text) } else { insert_plain(buffer, (a, b), &format!("\n{text}")) }
}

/// Inserts a citation, set apart from the words around it: a letter after the key would become part
/// of the key.
pub fn insert_citation(buffer: &mut String, (a, b): Sel, citation: &str) -> Sel {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let before = buffer.get(..byte_of(buffer, a)).and_then(|s| s.chars().next_back()).is_some_and(word);
    let after = buffer.get(byte_of(buffer, b)..).and_then(|s| s.chars().next()).is_some_and(word);
    let text = format!("{}{citation}{}", if before { " " } else { "" }, if after { " " } else { "" });
    insert_plain(buffer, (a, b), &text)
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

/// How a search compares. By default it ignores case and finds parts of words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

/// A query ready to be compared at places in a text.
struct Matcher {
    needle: Vec<char>,
    hay: Vec<char>,
    options: FindOptions,
}

fn in_a_word(letter: char) -> bool {
    letter.is_alphanumeric() || letter == '_'
}

impl Matcher {
    fn new(text: &str, query: &str, options: FindOptions) -> Self {
        let needle = if options.match_case { query.chars().collect() } else { query.chars().flat_map(char::to_lowercase).collect() };
        Self { needle, hay: text.chars().collect(), options }
    }

    fn at(&self, at: usize) -> bool {
        let Some(window) = self.hay.get(at..at + self.needle.len()) else { return false };
        let same = if self.options.match_case {
            window == self.needle.as_slice()
        } else {
            window.iter().zip(&self.needle).all(|(h, n)| h.to_lowercase().eq(std::iter::once(*n)))
        };
        same && (!self.options.whole_word || self.stands_alone(at))
    }

    /// Whether the match here has no more of its word on either side. A query's edge that's no letter,
    /// as in `#set`, has no word to run on into on that side.
    fn stands_alone(&self, at: usize) -> bool {
        let before = at.checked_sub(1).and_then(|i| self.hay.get(i)).copied();
        let after = self.hay.get(at + self.needle.len()).copied();
        let first_free = !self.needle.first().copied().is_some_and(in_a_word) || !before.is_some_and(in_a_word);
        let last_free = !self.needle.last().copied().is_some_and(in_a_word) || !after.is_some_and(in_a_word);
        first_free && last_free
    }
}

/// Every match of `query`, not overlapping, as char ranges, up to `limit`.
pub fn find_all(text: &str, query: &str, options: FindOptions, limit: usize) -> Vec<Sel> {
    let matcher = Matcher::new(text, query, options);
    let size = matcher.needle.len();
    if size == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 0;
    while at + size <= matcher.hay.len() && out.len() < limit {
        if matcher.at(at) {
            out.push((at, at + size));
            at += size;
        } else {
            at += 1;
        }
    }
    out
}

/// Replaces these matches, char ranges in order and not overlapping, by `with`, in one pass however many
/// there are. The selection stays on the same letters; inside a match, it goes to the end of what took
/// its place.
pub fn replace_matches(text: &mut String, (a, b): Sel, matches: &[Sel], with: &str) -> Sel {
    let added = with.chars().count();
    let moved = |c: usize| {
        let mut out = c;
        for &(start, end) in matches {
            if c >= end {
                out = out - (end - start) + added;
            } else if c > start {
                return out - (c - start) + added;
            } else {
                break;
            }
        }
        out
    };
    let selection = (moved(a), moved(b));
    let mut out = String::with_capacity(text.len());
    let mut remaining = matches.iter();
    let mut current = remaining.next();
    for (index, letter) in text.chars().enumerate() {
        if let Some(&(start, end)) = current {
            if index == start {
                out.push_str(with);
            }
            if (start..end).contains(&index) {
                if index + 1 == end {
                    current = remaining.next();
                }
                continue;
            }
        }
        out.push(letter);
    }
    *text = out;
    selection
}

/// The next match of `query` from char `from`, wrapping round the end of the text.
pub fn find(text: &str, query: &str, options: FindOptions, from: usize, backwards: bool) -> Option<Sel> {
    let matcher = Matcher::new(text, query, options);
    let size = matcher.needle.len();
    if size == 0 || matcher.hay.len() < size {
        return None;
    }
    let last = matcher.hay.len() - size;
    let from = from.min(last + 1);
    let found = if backwards {
        (0..from).rev().find(|i| matcher.at(*i)).or_else(|| (from..=last).rev().find(|i| matcher.at(*i)))
    } else {
        (from..=last).find(|i| matcher.at(*i)).or_else(|| (0..from).find(|i| matcher.at(*i)))
    };
    found.map(|i| (i, i + size))
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

    fn replaced(text: &str, query: &str, with: &str) -> String {
        let mut out = text.to_string();
        replace_matches(&mut out, (0, 0), &find_all(text, query, FindOptions::default(), usize::MAX), with);
        out
    }

    #[test]
    fn every_match_is_replaced_as_written_and_nothing_else() {
        assert_eq!(replaced("um Alvo e um alvo.", "alvo", "gol"), "um gol e um gol.");
        assert_eq!(replaced("açúcar é ação", "ção", "ções"), "açúcar é ações");
        assert_eq!(replaced("nada aqui", "xis", "y"), "nada aqui");
        assert_eq!(replaced("tirar isto", " isto", ""), "tirar");
        assert_eq!(replaced("a a", "a", "aa"), "aa aa", "what comes in isn't searched again");
        assert_eq!(replaced("x", "x", "$1 \\n #set"), "$1 \\n #set", "the replacement is plain text");
        assert_eq!(replaced("", "x", "y"), "");
        assert_eq!(replaced("x", "", "y"), "x");
    }

    #[test]
    fn a_search_can_match_case_and_whole_words() {
        let case = FindOptions { match_case: true, whole_word: false };
        let word = FindOptions { match_case: false, whole_word: true };
        let both = FindOptions { match_case: true, whole_word: true };
        let text = "Ana e banana, ANA e ana_b. ação Ação";
        assert_eq!(find_all(text, "ana", FindOptions::default(), 99).len(), 4);
        assert_eq!(find_all(text, "ana", case, 99), [(7, 10), (20, 23)]);
        assert_eq!(find_all(text, "ana", word, 99), [(0, 3), (14, 17)], "banana and ana_b are other words");
        assert_eq!(find_all(text, "Ana", both, 99), [(0, 3)]);
        assert_eq!(find_all(text, "ção", word, 99), [], "an accented letter is part of its word");
        assert_eq!(find_all(text, "Ação", case, 99).len(), 1);
        assert_eq!(find_all("#set page #setting", "#set", word, 99), [(0, 4)], "what isn't a letter has no word on that side");
        assert_eq!(find_all("(a) a", "(a)", word, 99), [(0, 3)]);
        assert_eq!(find(text, "ana", word, 1, false), Some((14, 17)));
        assert_eq!(find(text, "ana", word, 1, true), Some((0, 3)));
        assert_eq!(find(text, "ana", word, 0, true), Some((14, 17)), "going back wraps round too");
        assert_eq!(find("banana", "ana", word, 0, false), None);
        assert_eq!(find("", "a", both, 0, false), None);
        assert!(find_all("x", "", both, 99).is_empty());
    }

    #[test]
    fn replacing_keeps_the_selection_on_its_letters() {
        let mut text = "aa alvo bb alvo cc".to_string();
        let matches = find_all(&text, "alvo", FindOptions::default(), usize::MAX);
        let (a, b) = replace_matches(&mut text.clone(), (11, 16), &matches, "x");
        assert_eq!((a, b), (8, 10), "the start of a match, and the letter after it, stay where they are");
        let (a, _) = replace_matches(&mut text, (13, 13), &matches, "x");
        assert_eq!(text, "aa x bb x cc");
        assert_eq!(a, 9, "from inside a match to the end of what took its place");
        let mut text = "alvo fim".to_string();
        assert_eq!(replace_matches(&mut text, (5, 5), &[(0, 4)], "é"), (2, 2));
        let mut text = "um alvo".to_string();
        assert_eq!(replace_matches(&mut text, (0, 2), &[(3, 7)], "é"), (0, 2), "what's before stays");
    }

    #[test]
    fn a_huge_replacement_takes_no_time() {
        let mut text = "alvo ".repeat(200_000);
        let matches = find_all(&text, "alvo", FindOptions::default(), usize::MAX);
        let started = std::time::Instant::now();
        replace_matches(&mut text, (0, 0), &matches, "gol");
        assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
        assert_eq!(text.len(), 800_000);
    }

    #[test]
    fn an_argument_is_added_then_replaced() {
        let mut t = "ç\n".to_string();
        let s = set_argument(&mut t, (1, 1), "page", "paper", "\"a4\"");
        assert_eq!(t, "#set page(paper: \"a4\")\nç\n");
        assert_eq!(s, (24, 24));
        let s = set_argument(&mut t, s, "page", "paper", "\"a5\"");
        assert_eq!(t, "#set page(paper: \"a5\")\nç\n");
        assert_eq!(s, (24, 24));
    }

    #[test]
    fn setting_an_argument_keeps_the_rest_of_its_rule() {
        let set = |text: &str, function: &str, argument: &str, value: &str| {
            let mut buffer = text.to_string();
            set_argument(&mut buffer, (0, 0), function, argument, value);
            buffer
        };
        assert_eq!(
            set("#set page(paper: \"a4\", margin: 2.5cm, numbering: \"1\")\nT", "page", "paper", "\"a5\""),
            "#set page(paper: \"a5\", margin: 2.5cm, numbering: \"1\")\nT"
        );
        assert_eq!(
            set("#set par(justify: false, leading: 1.05em, first-line-indent: 1.25cm)\nT", "par", "justify", "true"),
            "#set par(justify: true, leading: 1.05em, first-line-indent: 1.25cm)\nT"
        );
    }

    #[test]
    fn the_document_font_is_read_wherever_the_rule_keeps_it() {
        assert_eq!(document_font("#set text(font: \"A\")\nT").as_deref(), Some("A"));
        assert_eq!(document_font("#set text(size: 11pt, font: \"B\")\nT").as_deref(), Some("B"));
        assert_eq!(document_font("#set text(font: (\"C\", \"D\"))\nT").as_deref(), Some("C"));
        assert_eq!(document_font("#set text(font: \"A\")\n#set text(font: \"E\")\nT").as_deref(), Some("E"), "the last rule decides");
        assert_eq!(document_font("// #set text(font: \"A\")\nT"), None);
        assert_eq!(document_font("#set text(font: fonte)\nT"), None);
    }

    #[test]
    fn a_setting_changes_the_rule_that_decides_it() {
        let mut t = "#set par(justify: true, leading: 1.05em, first-line-indent: 1.25cm)\nT".to_string();
        set_argument(&mut t, (0, 0), "par", "leading", "1.4em");
        assert_eq!(t, "#set par(justify: true, leading: 1.4em, first-line-indent: 1.25cm)\nT", "not a new rule above, which this one would override");
        let mut t = "#set text(size: 11pt)\nA\n#set text(font: \"X\", size: 12pt)\nB".to_string();
        set_argument(&mut t, (0, 0), "text", "font", "\"Y\"");
        assert_eq!(t, "#set text(size: 11pt)\nA\n#set text(font: \"Y\", size: 12pt)\nB");
    }

    const FIGURE: &str = "#figure(\n  image(\"a.png\"),\n  caption: [Legenda],\n) <fig>";

    #[test]
    fn an_image_call_is_found_with_its_block() {
        let text = format!("Antes\n\n{FIGURE}\n\nDepois #image(\"b.png\") dentro\n");
        let byte = text.find("image(\"a").unwrap();
        let found = image_call_at(&text, byte).unwrap();
        assert_eq!(&text[found.call.clone()], "image(\"a.png\")");
        assert_eq!(&text[found.block.clone().unwrap()], FIGURE, "with its label");
        let inline = image_call_at(&text, text.find("image(\"b").unwrap()).unwrap();
        assert_eq!(&text[inline.block.unwrap()], "#image(\"b.png\")");
        assert_eq!(image_call_at(&text, 2), None);
    }

    #[test]
    fn the_width_is_set_on_the_call() {
        let set = |text: &str| {
            let mut buffer = text.to_string();
            let byte = text.find("image").unwrap();
            set_image_width(&mut buffer, (0, 0), byte, "5cm");
            buffer
        };
        assert_eq!(set("#image(\"a.png\")"), "#image(\"a.png\", width: 5cm)");
        assert_eq!(set("#image(\"a.png\", width: 80%)"), "#image(\"a.png\", width: 5cm)");
        assert_eq!(set("#image(\"a.png\", height: 2cm)"), "#image(\"a.png\", width: 5cm)");
        assert_eq!(set("#image(\"a.png\", height: 2cm, fit: \"cover\")"), "#image(\"a.png\",  fit: \"cover\", width: 5cm)");
        assert_eq!(set("#image(\n  \"a.png\",\n)"), "#image(\n  \"a.png\", width: 5cm\n)");
    }

    #[test]
    fn a_block_moves_between_paragraphs_and_never_over_itself() {
        let text = format!("#set text(size: 11pt)\nUm\ndois\n\n{FIGURE}\n\nTrês\nquatro\n\n= Título\nCinco\n");
        let block = text.find("#figure").unwrap()..text.find("<fig>").unwrap() + 5;
        let moved = |target: &str, above: bool| {
            let mut buffer = text.clone();
            move_block(&mut buffer, block.clone(), text.find(target).unwrap(), above).map(|_| buffer)
        };
        // Below the second line of a paragraph goes after the paragraph, above it before.
        assert_eq!(moved("quatro", false).unwrap(), format!("#set text(size: 11pt)\nUm\ndois\n\n\nTrês\nquatro\n{FIGURE}\n\n= Título\nCinco\n"));
        assert_eq!(
            moved("dois", true).unwrap(),
            format!("#set text(size: 11pt)\n{FIGURE}\nUm\ndois\n\n\nTrês\nquatro\n\n= Título\nCinco\n"),
            "after the rule, not before it"
        );
        assert_eq!(moved("Cinco", false).unwrap(), format!("#set text(size: 11pt)\nUm\ndois\n\n\nTrês\nquatro\n\n= Título\nCinco\n{FIGURE}\n"));
    }

    #[test]
    fn a_block_at_the_end_of_a_text_with_no_newline_moves() {
        let mut text = "Um\n\n#image(\"a.png\")".to_string();
        let block = text.find('#').unwrap()..text.len();
        let at = move_block(&mut text, block, 0, true).unwrap();
        assert_eq!(text, "#image(\"a.png\")\nUm\n\n");
        assert_eq!(at, 0);
        // Right next to itself, it stays.
        let mut text = "Um\n\n#image(\"a.png\")\nDois\n".to_string();
        let block = text.find('#').unwrap()..text.find(")\n").unwrap() + 1;
        let target = text.find("Dois").unwrap();
        assert_eq!(move_block(&mut text, block, target, true), None);
        let mut text = "Um\n#image(\"a.png\")\nDois\n".to_string();
        let block = text.find('#').unwrap()..text.find(")\n").unwrap() + 1;
        assert_eq!(delete_block(&mut text, block), 3);
        assert_eq!(text, "Um\nDois\n");
    }

    #[test]
    fn the_margin_is_set_wherever_the_rule_keeps_it() {
        let set = |text: &str, at: usize| {
            let mut buffer = text.to_string();
            let sel = set_page_margin(&mut buffer, (at, at), "1cm");
            (buffer, sel.0)
        };
        assert_eq!(set("Texto", 2), ("#set page(margin: 1cm)\nTexto".to_string(), 25));
        assert_eq!(set("#set page(margin: 2.5cm)\nTexto", 26).0, "#set page(margin: 1cm)\nTexto");
        assert_eq!(set("#set page(margin: 2.5cm)\nTexto", 26).1, 24, "the caret stays on its letter");
        assert_eq!(
            set("#set page(paper: \"a5\", margin: (x: 2cm), numbering: \"1\")\nT", 0).0,
            "#set page(paper: \"a5\", margin: 1cm, numbering: \"1\")\nT"
        );
        assert_eq!(set("#set page(margin: 2cm)\nA\n#set page(margin: 3cm)\nB", 0).0, "#set page(margin: 2cm)\nA\n#set page(margin: 1cm)\nB");
        assert_eq!(set("// #set page(margin: 2cm)\nA", 0).0, "#set page(margin: 1cm)\n// #set page(margin: 2cm)\nA", "a comment is no rule");
        assert_eq!(set("#set page(paper: \"a5\")\nA", 0).0, "#set page(margin: 1cm)\n#set page(paper: \"a5\")\nA");
    }

    #[test]
    fn a_citation_stays_apart_from_the_words_around_it() {
        let cited = |text: &str, at: usize| {
            let mut buffer = text.to_string();
            insert_citation(&mut buffer, (at, at), "@k");
            buffer
        };
        assert_eq!(cited("", 0), "@k");
        assert_eq!(cited("Como disse", 10), "Como disse @k");
        assert_eq!(cited("Como disse ", 11), "Como disse @k");
        assert_eq!(cited("Como.", 4), "Como @k.");
        assert_eq!(cited("ação", 2), "aç @k ão");
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
        assert_eq!(plain_doc("["), "[");
        assert_eq!(plain_doc("ação [é](ü) ç"), "ação é ç");
    }

    #[test]
    fn every_match_is_found_without_overlap_and_within_the_limit() {
        assert_eq!(find_all("Ação ação", "AÇÃO", FindOptions::default(), 10), [(0, 4), (5, 9)]);
        assert_eq!(find_all("aaaa", "aa", FindOptions::default(), 10), [(0, 2), (2, 4)]);
        assert_eq!(find_all("aaaa", "a", FindOptions::default(), 2).len(), 2);
        assert!(find_all("abc", "", FindOptions::default(), 10).is_empty());
        assert!(find_all("a", "abc", FindOptions::default(), 10).is_empty());
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
        assert_eq!(find(t, "ação", FindOptions::default(), 0, false), Some((0, 4)));
        assert_eq!(find(t, "ação", FindOptions::default(), 1, false), Some((5, 9)));
        assert_eq!(find(t, "ação", FindOptions::default(), 11, false), Some((0, 4)));
        assert_eq!(find(t, "ação", FindOptions::default(), 5, true), Some((0, 4)));
        assert_eq!(find(t, "ação", FindOptions::default(), 0, true), Some((10, 14)));
        assert_eq!(find(t, "", FindOptions::default(), 0, false), None);
        assert_eq!(find("a", "abc", FindOptions::default(), 0, false), None);
    }

    #[test]
    fn hostile_table_sizes_are_clamped() {
        assert!(table_snippet(0, 0, false).contains("columns: 1"));
        assert!(table_snippet(9, 9999, true).contains("columns: 26"));
    }
}
