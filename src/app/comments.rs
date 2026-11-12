//! Comments in the window: their pane, their file beside the document, and sharing them in a session.
//! The source isn't touched: adding a comment doesn't make the document need saving.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{RichText, Ui};

use super::{App, Side};
use crate::collab::Session;
use crate::comments::{self, Book, MarkKind, Outcome};
use crate::compiler::{Job, PagePlace, PickRefusal, write_whole};
use crate::edit;
use crate::history;
use crate::i18n::tx;
use crate::theme::{self, Tokens};
use crate::tr;

/// What a drag on the pages makes, while reviewing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewTool {
    Strike,
    Highlight,
    Note,
}

/// A mark being written, on the passage picked on the pages.
struct Composer {
    tool: ReviewTool,
    range: std::ops::Range<usize>,
    quote: String,
    words: String,
    focus: bool,
}

/// A thread as a test or a picture sees it: its passage, whether it's still in the text, whether it's
/// resolved, and who said what.
pub type ThreadView = (String, bool, bool, Vec<(String, String)>);

/// A file bigger than this is no list of comments.
const MOST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Comments {
    pub(super) book: Book,
    /// The document these are the comments of.
    path: Option<PathBuf>,
    /// There's something the file doesn't have yet.
    unwritten: bool,
    /// The document had unsaved changes in the last frame: saving moves the passages in the file.
    was_dirty: bool,
    /// As host, this session has them.
    shared: bool,
    /// What the session now says about them, to take in with the text it came with.
    pub(super) pending: Option<std::collections::BTreeMap<String, String>>,
    new_comment: String,
    focus_new: bool,
    /// Reviewing on the pages, with this tool.
    pub(super) tool: Option<ReviewTool>,
    /// Where a drag on the pages began: its page, and the point there.
    pub(super) drag_from: Option<(usize, egui::Pos2)>,
    /// A pick under way on the compiler's thread, and the tool it's for.
    picking: Option<(u64, ReviewTool)>,
    composer: Option<Composer>,
    /// Why the last pick couldn't be marked.
    refusal: Option<PickRefusal>,
    /// The threads on the pages, by what was last asked: with their ids, in the order asked.
    asked_places: Vec<(String, std::ops::Range<usize>)>,
    asked_at: u64,
    places: Vec<PagePlace>,
    replies: HashMap<String, String>,
    show_resolved: bool,
}

fn file_for(document: &Path) -> PathBuf {
    let mut name = document.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    name.push(".comments.json");
    document.with_file_name(name)
}

/// What the file has, or nothing with a notice. A damaged one is moved aside, never written over.
fn read_file(document: &Path) -> (std::collections::BTreeMap<String, String>, Option<String>) {
    let file = file_for(document);
    let Ok(meta) = std::fs::metadata(&file) else { return Default::default() };
    let parsed = (meta.len() <= MOST_BYTES).then(|| std::fs::read_to_string(&file).ok()).flatten().and_then(|text| Book::entries_from_file(&text));
    if let Some(entries) = parsed {
        return (entries, None);
    }
    let mut aside = file.clone().into_os_string();
    aside.push(".damaged");
    let _ = std::fs::rename(&file, PathBuf::from(&aside));
    (Default::default(), Some(tr!("the-comments-file-was-damaged", path = PathBuf::from(aside).display().to_string())))
}

impl App {
    /// Each frame: the right document's comments, in step with its text, the session and the file.
    pub(super) fn comments_tick(&mut self) {
        if self.comments.path != self.path {
            self.comments_changed_document();
        }
        if let Some(entries) = self.comments.pending.take() {
            self.receive_comments(entries);
        }
        let Some(session) = &mut self.session else {
            self.comments.shared = false;
            self.comments_follow_and_write();
            return;
        };
        if session.is_host() && !self.comments.shared {
            self.comments.shared = true;
            for (key, value) in self.comments.book.entries() {
                session.set_comment(key, value);
            }
        }
        self.comments_follow_and_write();
    }

    fn comments_follow_and_write(&mut self) {
        self.comments.book.follow(&self.text);
        let saved = self.comments.was_dirty && !self.dirty;
        self.comments.was_dirty = self.dirty;
        let any = !self.comments.book.threads().is_empty();
        if self.comments.unwritten || (saved && any) {
            self.write_comments();
        }
    }

    /// A new document with no file: the last one's comments stay with it. Going from no file to no
    /// file isn't a change of path, so the frame wouldn't see it, and Save As would take them along.
    pub(super) fn comments_start_afresh(&mut self) {
        self.comments.path = self.path.clone();
        self.comments.book.replace_entries(Default::default(), &self.text);
        self.comments.unwritten = false;
        self.comments.replies.clear();
    }

    fn comments_changed_document(&mut self) {
        let path = self.path.clone();
        let same_document = self.comments.book.known_text() == self.text;
        let has_file = path.as_deref().is_some_and(|p| file_for(p).exists());
        self.comments.path = path.clone();
        // Save as: the same text, somewhere with no comments of its own. They go along.
        if same_document && !has_file && path.is_some() {
            self.comments.unwritten = !self.comments.book.threads().is_empty();
            return;
        }
        let (entries, notice) = path.as_deref().map(read_file).unwrap_or_default();
        self.comments.book.replace_entries(entries, &self.text);
        self.comments.unwritten = false;
        self.comments.replies.clear();
        if let Some(notice) = notice {
            self.status = notice;
        }
    }

    fn write_comments(&mut self) {
        self.comments.unwritten = false;
        let Some(document) = &self.path else { return };
        let file = file_for(document);
        if self.comments.book.threads().is_empty() {
            let _ = std::fs::remove_file(file);
        } else if let Err(error) = write_whole(&file, self.comments.book.to_file().as_bytes()) {
            self.status = tr!("the-comments-could-not-be-saved", error = error.to_string());
        }
    }

    /// Takes in what a session now says about them.
    fn receive_comments(&mut self, entries: std::collections::BTreeMap<String, String>) {
        self.comments.book.replace_entries(entries, &self.text);
        if self.session.as_ref().is_some_and(Session::is_host) {
            self.comments.unwritten = true;
        }
    }

    fn share_comment_entries(&mut self, entries: Vec<(String, String)>) {
        if let Some(session) = &mut self.session {
            for (key, value) in &entries {
                session.set_comment(key, value);
            }
        }
        if !entries.is_empty() {
            self.comments.unwritten = true;
        }
    }

    /// Turns reviewing on the pages on or off, with this tool. The pane shows what comes of it.
    pub fn set_review_tool(&mut self, tool: Option<ReviewTool>) {
        self.comments.tool = tool;
        self.comments.drag_from = None;
        if tool.is_some() {
            self.side = Some(Side::Comments);
        }
    }

    pub fn review_tool(&self) -> Option<ReviewTool> {
        self.comments.tool
    }

    /// Asks what's between two points of a page, as a drag on it does.
    pub fn pick_on_page(&mut self, page: usize, from: [f64; 2], to: [f64; 2]) {
        let Some(tool) = self.comments.tool else { return };
        self.variants_asked += 1;
        self.comments.picking = Some((self.variants_asked, tool));
        self.comments.refusal = None;
        self.compiler.send(Job::ReviewPick { request: self.variants_asked, page, from, to });
    }

    pub(super) fn receive_pick(&mut self, request: u64, result: Result<std::ops::Range<usize>, PickRefusal>) {
        let Some((asked, tool)) = self.comments.picking else { return };
        if asked != request {
            return;
        }
        self.comments.picking = None;
        match result {
            Ok(range) => {
                // A note changes nothing, so it can sit on any passage; a mark rewrites what it's on.
                let range = if tool == ReviewTool::Note { Some(range) } else { edit::markable(&self.text, range) };
                let Some(range) = range else {
                    self.comments.refusal = Some(PickRefusal::Structure);
                    return;
                };
                let quote = self.text.get(range.clone()).unwrap_or_default().to_string();
                if quote.trim().is_empty() {
                    self.comments.refusal = Some(PickRefusal::Nothing);
                } else {
                    self.comments.composer = Some(Composer { tool, range, quote, words: String::new(), focus: true });
                }
            }
            Err(refusal) => self.comments.refusal = Some(refusal),
        }
    }

    /// The mark being written: its passage and tool, while there is one.
    pub fn composing_mark(&self) -> Option<(String, ReviewTool)> {
        self.comments.composer.as_ref().map(|c| (c.quote.clone(), c.tool))
    }

    pub fn review_refusal(&self) -> Option<PickRefusal> {
        self.comments.refusal
    }

    /// Makes the mark being written, with these words: what replaces a struck passage, or a note.
    pub fn finish_mark(&mut self, words: &str) {
        let Some(composer) = self.comments.composer.take() else { return };
        // Text that moved under the mark while it was written isn't what was picked.
        let range = self.comments.book.relocated_range(&self.text, &composer.quote, composer.range);
        let author = self.comment_author();
        let now = history::now_ms();
        let made = match composer.tool {
            ReviewTool::Strike => self.comments.book.start_mark(&self.text, range, &author, MarkKind::Strike, words, "", now),
            ReviewTool::Highlight => self.comments.book.start_mark(&self.text, range, &author, MarkKind::Highlight, "", words, now),
            ReviewTool::Note => self.comments.book.start(&self.text, range, &author, words, now),
        };
        if made.is_empty() {
            self.status = tx("this-passage-cant-be-marked").into();
        }
        self.share_comment_entries(made);
    }

    /// Whether this thread is a mark that can still be applied.
    pub fn mark_applies(&self, index: usize) -> bool {
        self.comments.book.threads().get(index).is_some_and(|t| self.comments.book.applicable(&t.id).is_some())
    }

    /// The outcome of a mark, once it has one.
    pub fn mark_outcome(&self, index: usize) -> Option<Outcome> {
        self.comments.book.threads().get(index).and_then(|t| t.outcome)
    }

    /// Applies a mark, as one undo step, if it still says what it did.
    pub fn accept_mark(&mut self, ctx: &egui::Context, index: usize) {
        let Some(id) = self.comments.book.threads().get(index).map(|t| t.id.clone()) else { return };
        let Some((thread, range)) = self.comments.book.applicable(&id) else { return };
        if !comments::mark_is_safe(&thread, range.clone(), &self.text) {
            self.status = tx("this-passage-cant-be-marked").into();
            return;
        }
        self.edit(ctx, |text, sel| comments::apply_mark(&thread, range, text, sel));
        self.comments.book.follow(&self.text);
        let made = self.comments.book.conclude(&id, Outcome::Accepted);
        self.share_comment_entries(made);
    }

    /// Closes a mark without touching the text.
    pub fn decline_mark(&mut self, index: usize) {
        let Some(id) = self.comments.book.threads().get(index).map(|t| t.id.clone()) else { return };
        let made = self.comments.book.conclude(&id, Outcome::Declined);
        self.share_comment_entries(made);
    }

    /// Where the open threads are on the pages, once asked.
    pub fn comment_places(&self) -> &[PagePlace] {
        &self.comments.places
    }

    /// Keeps the places on the pages up to date, while they're on show.
    pub(super) fn ask_for_places(&mut self) {
        if self.side != Some(Side::Comments) && self.comments.tool.is_none() {
            return;
        }
        let book = &self.comments.book;
        let open: Vec<(String, std::ops::Range<usize>)> =
            book.threads().iter().filter(|t| !t.resolved).filter_map(|t| Some((t.id.clone(), book.place(&t.id)?))).collect();
        if open == self.comments.asked_places && self.comments.asked_at == self.compiles {
            return;
        }
        self.comments.asked_at = self.compiles;
        self.comments.places.clear();
        let ranges = open.iter().map(|(_, range)| range.clone()).collect();
        self.comments.asked_places = open;
        self.compiler.send(Job::Places { ranges });
    }

    pub(super) fn receive_places(&mut self, places: Vec<PagePlace>) {
        self.comments.places = places;
    }

    /// The marks and comments on one page, drawn over it, with what they say under the pointer.
    pub(super) fn comments_on_page(&self, ui: &Ui, page: usize, rect: egui::Rect, zoom: f32, t: &Tokens) {
        if self.side != Some(Side::Comments) && self.comments.tool.is_none() {
            return;
        }
        for place in self.comments.places.iter().filter(|p| p.page == page) {
            let Some((id, _)) = self.comments.asked_places.get(place.index) else { continue };
            let Some(thread) = self.comments.book.threads().iter().find(|t| t.id == *id) else { continue };
            let at = rect.min + egui::vec2(place.x, place.y) * zoom;
            let mark = egui::Rect::from_min_size(at, egui::vec2(place.width, place.height) * zoom);
            let painter = ui.painter();
            match thread.mark {
                Some(MarkKind::Strike) => {
                    painter.hline(mark.x_range(), mark.center().y, egui::Stroke::new(1.6, t.danger));
                }
                Some(MarkKind::Highlight) => {
                    painter.rect_filled(mark, 1.0, t.warning.gamma_multiply(0.35));
                }
                None => {
                    painter.rect_filled(mark, 1.0, t.accent.gamma_multiply(0.12));
                    painter.hline(mark.x_range(), mark.bottom(), egui::Stroke::new(1.0, t.accent));
                }
            }
            let said = thread.messages.first().map(|m| m.text.as_str());
            let tip = match (thread.mark, thread.with.is_empty(), said) {
                (Some(MarkKind::Strike), false, _) => Some(tr!("replace-by", with = thread.with.clone())),
                (_, _, Some(text)) => Some(text.to_string()),
                _ => None,
            };
            if let Some(tip) = tip {
                ui.interact(mark, ui.scope_id().with(("mark", page, place.index, place.x.to_bits(), place.y.to_bits())), egui::Sense::hover())
                    .on_hover_text(tip);
            }
        }
    }

    /// What's being written, at the top of the pane.
    fn mark_composer(&mut self, ui: &mut Ui, t: &Tokens) {
        let Some(composer) = &mut self.comments.composer else { return };
        let (title, hint) = match composer.tool {
            ReviewTool::Strike => (tx("strike-out"), tx("replace-by-optional")),
            ReviewTool::Highlight => (tx("highlight-mark"), tx("write-a-comment")),
            ReviewTool::Note => (tx("note"), tx("write-a-comment")),
        };
        ui.label(RichText::new(title).font(theme::ui_bold(12.5)));
        let quote: String = composer.quote.chars().take(160).collect();
        ui.add(egui::Label::new(RichText::new(quote).font(egui::FontId::monospace(11.5)).color(t.accent)).wrap());
        let field = ui.add(egui::TextEdit::multiline(&mut composer.words).desired_rows(2).desired_width(f32::INFINITY).hint_text(hint));
        if std::mem::take(&mut composer.focus) {
            field.request_focus();
        }
        let ready = composer.tool != ReviewTool::Note || !composer.words.trim().is_empty();
        let (mut make, mut cancel) = (false, false);
        ui.horizontal(|ui| {
            make = ui.add_enabled(ready, egui::Button::new(tx("mark"))).clicked();
            cancel = ui.button(tx("cancel")).clicked();
        });
        if make {
            let words = composer.words.clone();
            self.finish_mark(&words);
        } else if cancel {
            self.comments.composer = None;
        }
        ui.separator();
    }

    /// What this person is called in a comment.
    fn comment_author(&self) -> String {
        if self.user_name.trim().is_empty() { tx("someone").to_string() } else { self.user_name.clone() }
    }

    /// Comments on the selection, as the pane's button does. True if it had something to go on.
    pub fn comment_on_selection(&mut self, ctx: &egui::Context, body: &str) -> bool {
        let (a, b) = self.selection(ctx);
        let range = edit::byte_of(&self.text, a)..edit::byte_of(&self.text, b);
        let author = self.comment_author();
        let made = self.comments.book.start(&self.text, range, &author, body, history::now_ms());
        let any = !made.is_empty();
        self.share_comment_entries(made);
        any
    }

    pub fn reply_to_comment(&mut self, index: usize, body: &str) {
        let Some(id) = self.comments.book.threads().get(index).map(|t| t.id.clone()) else { return };
        let author = self.comment_author();
        let made = self.comments.book.reply(&id, &author, body, history::now_ms());
        self.share_comment_entries(made);
    }

    pub fn resolve_comment(&mut self, index: usize, resolved: bool) {
        let Some(id) = self.comments.book.threads().get(index).map(|t| t.id.clone()) else { return };
        let made = self.comments.book.set_resolved(&id, resolved);
        self.share_comment_entries(made);
    }

    pub fn delete_comment(&mut self, index: usize) {
        let Some(id) = self.comments.book.threads().get(index).map(|t| t.id.clone()) else { return };
        let made = self.comments.book.delete(&id);
        self.share_comment_entries(made);
    }

    pub fn comment_threads(&self) -> Vec<ThreadView> {
        let book = &self.comments.book;
        book.threads()
            .iter()
            .map(|t| {
                let said = t.messages.iter().map(|m| (m.author.clone(), m.text.clone())).collect();
                (t.quote.clone(), book.place(&t.id).is_some(), t.resolved, said)
            })
            .collect()
    }

    /// Selects a thread's passage in the source and shows it on the pages.
    pub fn go_to_comment(&mut self, ctx: &egui::Context, index: usize) {
        let Some(range) = self.comments.book.threads().get(index).and_then(|t| self.comments.book.place(&t.id)) else { return };
        self.set_selection(ctx, (edit::char_of(&self.text, range.start), edit::char_of(&self.text, range.end)));
        self.scroll_caret = true;
        self.want_focus = true;
        self.compiler.send(crate::compiler::Job::Locate { byte: range.start });
    }

    /// Opens the pane to comment on the selection.
    pub(super) fn start_comment(&mut self) {
        self.side = Some(Side::Comments);
        self.comments.focus_new = true;
    }

    pub(super) fn comments_in_use(&self) -> bool {
        !self.comments.book.threads().is_empty()
    }

    pub(super) fn open_comments(&self) -> usize {
        self.comments.book.open_count()
    }

    pub(super) fn pane_comments(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        if self.comments.tool.is_some() {
            ui.colored_label(t.text_dim, tx("drag-over-the-pages-to-mark"));
        }
        if let Some(refusal) = self.comments.refusal {
            let words = match refusal {
                PickRefusal::Generated => tx("this-comes-from-the-documents-code"),
                PickRefusal::Structure => tx("this-passage-crosses-the-documents-structure"),
                PickRefusal::Nothing => tx("there-is-no-text-there"),
            };
            ui.add(egui::Label::new(RichText::new(words).color(t.warning_ink)).wrap());
        }
        self.mark_composer(ui, t);
        let waiting = self.session.as_ref().is_some_and(Session::is_waiting);
        let selection = self.selection(&ctx);
        let selected = selection.0 != selection.1 && !edit::selected(&self.text, selection).trim().is_empty();
        let field =
            egui::TextEdit::multiline(&mut self.comments.new_comment).desired_rows(2).desired_width(f32::INFINITY).hint_text(tx("write-a-comment"));
        let field = ui.add_enabled(!waiting, field);
        if std::mem::take(&mut self.comments.focus_new) {
            field.request_focus();
        }
        let can = selected && !waiting && !self.comments.new_comment.trim().is_empty();
        let mut comment = false;
        ui.horizontal(|ui| {
            comment = ui.add_enabled(can, egui::Button::new(tx("comment"))).clicked();
            if !selected {
                ui.colored_label(t.text_dim, tx("select-a-passage-to-comment"));
            }
        });
        if comment {
            let body = std::mem::take(&mut self.comments.new_comment);
            self.comment_on_selection(&ctx, &body);
        }
        ui.add_space(6.0);
        let threads = self.comments.book.threads().to_vec();
        let resolved = threads.iter().filter(|t| t.resolved).count();
        if threads.is_empty() {
            ui.colored_label(t.text_dim, tx("no-comments-yet"));
            ui.add_space(6.0);
        }
        if resolved > 0 {
            ui.checkbox(&mut self.comments.show_resolved, tr!("show-resolved", n = resolved));
            ui.add_space(4.0);
        }
        ui.colored_label(t.text_dim, tx("comments-stay-beside-the-document"));
        ui.add_space(8.0);
        for (index, thread) in threads.iter().enumerate() {
            if thread.resolved && !self.comments.show_resolved {
                continue;
            }
            ui.push_id(&thread.id, |ui| self.comment_thread(ui, &ctx, index, thread, t));
            ui.separator();
        }
    }

    fn comment_thread(&mut self, ui: &mut Ui, ctx: &egui::Context, index: usize, thread: &crate::comments::Thread, t: &Tokens) {
        if let Some(mark) = thread.mark {
            let what = match (mark, thread.with.is_empty()) {
                (MarkKind::Strike, true) => tx("strike-out").to_string(),
                (MarkKind::Strike, false) => tr!("replace-by", with = thread.with.clone()),
                (MarkKind::Highlight, _) => tx("highlight-mark").to_string(),
            };
            ui.add(egui::Label::new(RichText::new(what).font(theme::ui_bold(12.5))).wrap());
        }
        let placed = self.comments.book.place(&thread.id).is_some();
        let quote: String = thread.quote.chars().take(120).collect();
        let quote = RichText::new(quote).font(egui::FontId::monospace(11.5)).color(if placed { t.accent } else { t.text_dim });
        let label = ui.add(egui::Label::new(quote).wrap().sense(egui::Sense::click()));
        if placed && label.on_hover_text(tx("go-to-the-passage")).clicked() {
            self.go_to_comment(ctx, index);
        }
        if !placed {
            ui.colored_label(t.warning_ink, tx("this-passage-is-no-longer-in-the-text"));
        } else if thread.mark.is_some() && thread.outcome.is_none() && !self.comments.book.still_applies(thread) {
            ui.add(egui::Label::new(RichText::new(tx("the-text-changed-since-this-mark")).color(t.warning_ink)).wrap());
        }
        match thread.outcome {
            Some(Outcome::Accepted) => {
                ui.colored_label(t.text_dim, tx("accepted"));
            }
            Some(Outcome::Declined) => {
                ui.colored_label(t.text_dim, tx("declined"));
            }
            None => {}
        }
        let now = history::now_ms();
        for message in &thread.messages {
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&message.author).font(theme::ui_bold(12.5)));
                ui.label(RichText::new(history::describe(message.stamp, now)).font(theme::ui_font(11.0)).color(t.text_dim));
            });
            ui.add(egui::Label::new(&message.text).wrap());
        }
        ui.add_space(4.0);
        let reply = self.comments.replies.entry(thread.id.clone()).or_default();
        ui.add(egui::TextEdit::singleline(reply).desired_width(f32::INFINITY).hint_text(tx("reply")));
        let can_reply = !reply.trim().is_empty();
        let (mut send, mut resolve, mut delete, mut accept, mut decline) = (false, false, false, false, false);
        let applies = self.comments.book.applicable(&thread.id).is_some();
        let pending_mark = thread.mark.is_some() && thread.outcome.is_none();
        ui.horizontal_wrapped(|ui| {
            send = ui.add_enabled(can_reply, egui::Button::new(tx("send-the-reply"))).clicked();
            if pending_mark {
                accept = ui.add_enabled(applies, egui::Button::new(tx("accept"))).clicked();
                decline = ui.button(tx("decline")).clicked();
            } else if thread.mark.is_none() || thread.resolved {
                resolve = ui.button(if thread.resolved { tx("reopen") } else { tx("resolve") }).clicked();
            }
            // A thread goes only once it's resolved: two steps, for what can't be undone.
            delete = thread.resolved && ui.button(tx("delete")).clicked();
        });
        if accept {
            self.accept_mark(ctx, index);
        }
        if decline {
            self.decline_mark(index);
        }
        if send {
            let body = self.comments.replies.remove(&thread.id).unwrap_or_default();
            self.reply_to_comment(index, &body);
        }
        if resolve {
            self.resolve_comment(index, !thread.resolved);
        }
        if delete {
            self.delete_comment(index);
        }
    }
}
