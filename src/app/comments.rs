//! Comments in the window: their pane, their file beside the document, and sharing them in a session.
//! The source isn't touched: adding a comment doesn't make the document need saving.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{RichText, Ui};

use super::{App, Side};
use crate::collab::Session;
use crate::comments::Book;
use crate::compiler::write_whole;
use crate::edit;
use crate::history;
use crate::i18n::tx;
use crate::theme::{self, Tokens};
use crate::tr;

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
        let placed = self.comments.book.place(&thread.id).is_some();
        let quote: String = thread.quote.chars().take(120).collect();
        let quote = RichText::new(quote).font(egui::FontId::monospace(11.5)).color(if placed { t.accent } else { t.text_dim });
        let label = ui.add(egui::Label::new(quote).wrap().sense(egui::Sense::click()));
        if placed && label.on_hover_text(tx("go-to-the-passage")).clicked() {
            self.go_to_comment(ctx, index);
        }
        if !placed {
            ui.colored_label(t.warning_ink, tx("this-passage-is-no-longer-in-the-text"));
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
        let (mut send, mut resolve, mut delete) = (false, false, false);
        ui.horizontal_wrapped(|ui| {
            send = ui.add_enabled(can_reply, egui::Button::new(tx("send-the-reply"))).clicked();
            resolve = ui.button(if thread.resolved { tx("reopen") } else { tx("resolve") }).clicked();
            // A thread goes only once it's resolved: two steps, for what can't be undone.
            delete = thread.resolved && ui.button(tx("delete")).clicked();
        });
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
