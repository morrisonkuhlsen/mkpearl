//! The Documentation pane: lessons, a searchable reference, and examples that can be edited and show
//! their result at once.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

use egui::text::LayoutJob;
use egui::{FontId, RichText, TextFormat, TextureHandle, TextureOptions, Ui, vec2};

use super::{App, Side};
use crate::compiler::{Job, Pixels};
use crate::edit;
use crate::guide::{self, Block, Hit};
use crate::i18n::tx;
use crate::icons::Icon;
use crate::recent;
use crate::theme::Tokens;
use crate::tr;
use crate::widgets::icon_button_mono;

const READ_FILE: &str = "guide.txt";
/// How long after a keystroke an edited example runs.
const SETTLE: f64 = 0.25;
/// Results kept for examples, before the oldest set goes.
const MAX_RENDERS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Page {
    #[default]
    Home,
    Functions,
    Lesson(usize),
    Function(usize),
}

#[derive(Default)]
pub(super) struct Docs {
    search: String,
    page: Page,
    /// Where to go back to.
    came_from: Vec<Page>,
    /// What's been done to each example on show, by page and place.
    edited: HashMap<(Page, usize), String>,
    edited_at: f64,
    pub(super) renders: HashMap<u64, Result<(TextureHandle, [usize; 2]), String>>,
    pub(super) asked: HashSet<u64>,
    /// Lessons read, by number.
    read: HashSet<usize>,
    scroll_to_top: bool,
}

impl Docs {
    pub(super) fn load() -> Self {
        let read =
            recent::read_setting(READ_FILE).map_or_else(HashSet::new, |text| text.lines().filter_map(|line| line.trim().parse().ok()).collect());
        Self { read, ..Self::default() }
    }

    pub(super) fn receive(&mut self, ctx: &egui::Context, request: u64, result: Result<Pixels, String>) {
        if self.renders.len() >= MAX_RENDERS {
            self.renders.clear();
            self.asked.clear();
        }
        let rendered = result.map(|pixels| {
            let image = egui::ColorImage::from_rgba_unmultiplied(pixels.size, &pixels.rgba);
            (ctx.load_texture(format!("example-{request:x}"), image, TextureOptions::LINEAR), pixels.size)
        });
        self.asked.insert(request);
        self.renders.insert(request, rendered);
    }

    fn open(&mut self, page: Page) {
        self.came_from.push(self.page);
        self.page = page;
        self.search.clear();
        self.scroll_to_top = true;
        if let Page::Lesson(i) = page
            && self.read.insert(i)
        {
            let mut read: Vec<usize> = self.read.iter().copied().collect();
            read.sort_unstable();
            let text: Vec<String> = read.iter().map(usize::to_string).collect();
            recent::write_setting(READ_FILE, &text.join("\n"));
        }
    }

    fn back(&mut self) {
        self.page = self.came_from.pop().unwrap_or_default();
        self.scroll_to_top = true;
    }
}

pub(super) fn hash_of(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Text with its `code` parts in the code font, wrapped to the pane.
fn rich_text(ui: &mut Ui, text: &str, color: egui::Color32, t: &Tokens) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    for (part, code) in guide::inline_parts(text) {
        let format = if code {
            TextFormat { font_id: FontId::monospace(12.0), color: t.syntax.keyword, background: t.hover, ..TextFormat::default() }
        } else {
            TextFormat { font_id: crate::theme::ui_font(13.0), color, ..TextFormat::default() }
        };
        job.append(part, 0.0, format);
    }
    ui.label(job);
}

impl App {
    pub(super) fn pane_docs(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let field = egui::TextEdit::singleline(&mut self.docs.search).hint_text(tx("search-the-documentation")).desired_width(f32::INFINITY);
        ui.add(field);
        ui.add_space(6.0);
        if std::mem::take(&mut self.docs.scroll_to_top) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        if !self.docs.search.trim().is_empty() {
            self.docs_results(ui, t);
            return;
        }
        match self.docs.page {
            Page::Home => self.docs_home(ui, t),
            Page::Functions => self.docs_functions(ui, t),
            Page::Lesson(i) => {
                let Some(lesson) = guide::lessons().get(i) else {
                    self.docs.page = Page::Home;
                    return;
                };
                self.docs_header(ui, t, &lesson.title, Some(&lesson.category));
                self.docs_blocks(ui, &ctx, t, Page::Lesson(i), &lesson.blocks);
                self.docs_lesson_steps(ui, i);
            }
            Page::Function(i) => {
                let Some(function) = guide::functions().get(i) else {
                    self.docs.page = Page::Home;
                    return;
                };
                self.docs_header(ui, t, &function.name, Some(&function.title));
                ui.colored_label(t.text_dim, tx("the-reference-is-in-english"));
                ui.add_space(4.0);
                self.docs_blocks(ui, &ctx, t, Page::Function(i), &function.blocks);
            }
        }
    }

    fn docs_header(&mut self, ui: &mut Ui, t: &Tokens, title: &str, above: Option<&str>) {
        ui.horizontal(|ui| {
            if icon_button_mono(ui, Icon::Back, t.text_dim, tx("back"), vec2(24.0, 22.0)).clicked() {
                self.docs.back();
            }
            if let Some(above) = above {
                ui.colored_label(t.text_dim, above);
            }
        });
        ui.label(RichText::new(title).font(crate::theme::ui_bold(16.0)).color(t.text));
        ui.add_space(4.0);
    }

    fn docs_home(&mut self, ui: &mut Ui, t: &Tokens) {
        let lessons = guide::lessons();
        let read = lessons.iter().enumerate().filter(|(i, _)| self.docs.read.contains(i)).count();
        ui.colored_label(t.text_dim, tr!("lessons-read", read = read.to_string(), total = lessons.len().to_string()));
        ui.add(egui::ProgressBar::new(read as f32 / lessons.len().max(1) as f32).desired_height(4.0));
        ui.add_space(4.0);
        let next = (0..lessons.len()).find(|i| !self.docs.read.contains(i));
        if let Some(lesson) = next.and_then(|i| lessons.get(i).map(|l| (i, l)))
            && ui.button(tr!("continue-with", title = lesson.1.title.clone())).clicked()
        {
            self.docs.open(Page::Lesson(lesson.0));
        }
        ui.add_space(6.0);
        let mut category = "";
        let mut chosen = None;
        for (i, lesson) in lessons.iter().enumerate() {
            if lesson.category != category {
                category = &lesson.category;
                ui.add_space(4.0);
                ui.label(RichText::new(category).font(crate::theme::ui_bold(13.0)).color(t.text));
            }
            let mark = if self.docs.read.contains(&i) { "✓ " } else { "   " };
            let button = egui::Button::new(RichText::new(format!("{mark}{}", lesson.title)).color(t.text)).frame_when_inactive(false).truncate();
            if ui.add(button).clicked() {
                chosen = Some(Page::Lesson(i));
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new(tx("function-reference")).font(crate::theme::ui_bold(13.0)).color(t.text));
        ui.colored_label(t.text_dim, tx("the-reference-is-in-english"));
        if ui.button(tr!("n-functions", count = guide::functions().len().to_string())).clicked() {
            chosen = Some(Page::Functions);
        }
        if let Some(page) = chosen {
            self.docs.open(page);
        }
    }

    fn docs_functions(&mut self, ui: &mut Ui, t: &Tokens) {
        self.docs_header(ui, t, tx("function-reference"), None);
        let mut chosen = None;
        for (i, function) in guide::functions().iter().enumerate() {
            let button = egui::Button::new(RichText::new(&function.name).monospace().color(t.text)).frame_when_inactive(false).truncate();
            if ui.add(button).on_hover_text(&function.title).clicked() {
                chosen = Some(Page::Function(i));
            }
        }
        if let Some(page) = chosen {
            self.docs.open(page);
        }
    }

    fn docs_results(&mut self, ui: &mut Ui, t: &Tokens) {
        let hits = guide::search(&self.docs.search);
        if hits.is_empty() {
            ui.colored_label(t.text_dim, tx("no-results"));
            return;
        }
        let mut chosen = None;
        for hit in hits {
            let (name, kind, page, code) = match hit {
                Hit::Lesson(i) => {
                    let Some(lesson) = guide::lessons().get(i) else { continue };
                    (lesson.title.as_str(), tx("lesson"), Page::Lesson(i), false)
                }
                Hit::Function(i) => {
                    let Some(function) = guide::functions().get(i) else { continue };
                    (function.name.as_str(), tx("function"), Page::Function(i), true)
                }
            };
            let text = if code { RichText::new(name).monospace() } else { RichText::new(name) };
            if ui.add(egui::Button::new(text.color(t.text)).frame_when_inactive(false).truncate()).clicked() {
                chosen = Some(page);
            }
            ui.label(RichText::new(kind).small().color(t.text_dim));
            ui.add_space(2.0);
        }
        if let Some(page) = chosen {
            self.docs.open(page);
        }
    }

    fn docs_lesson_steps(&mut self, ui: &mut Ui, i: usize) {
        ui.add_space(10.0);
        let mut go = None;
        ui.horizontal(|ui| {
            if i > 0 && ui.button(tx("previous-lesson")).clicked() {
                go = Some(i - 1);
            }
            if i + 1 < guide::lessons().len() && ui.button(tx("next-lesson")).clicked() {
                go = Some(i + 1);
            }
        });
        if let Some(next) = go {
            self.docs.open(Page::Lesson(next));
        }
    }

    fn docs_blocks(&mut self, ui: &mut Ui, ctx: &egui::Context, t: &Tokens, page: Page, blocks: &[Block]) {
        for (place, block) in blocks.iter().enumerate() {
            match block {
                Block::Heading(text) => {
                    ui.add_space(6.0);
                    ui.label(RichText::new(text).font(crate::theme::ui_bold(14.0)).color(t.text));
                }
                Block::Text(text) => rich_text(ui, text, t.text, t),
                Block::Code(code) => {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(code).monospace().size(12.0).color(t.text)).wrap());
                    });
                    if ui.small_button(tx("insert-at-the-caret")).clicked() {
                        self.docs_insert(ctx, code);
                    }
                }
                Block::Try(text) => {
                    egui::Frame::new().fill(t.accent_soft).inner_margin(6.0).corner_radius(3.0).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        rich_text(ui, text, t.text, t);
                    });
                }
                Block::Example(original) => self.docs_example(ui, ctx, t, (page, place), original),
            }
            ui.add_space(5.0);
        }
    }

    /// An example: its code, which can be edited, and what it makes, kept up to date.
    fn docs_example(&mut self, ui: &mut Ui, ctx: &egui::Context, t: &Tokens, key: (Page, usize), original: &str) {
        let now = ctx.input(|i| i.time);
        let mut text = self.docs.edited.get(&key).cloned().unwrap_or_else(|| original.to_string());
        let rows = text.lines().count().clamp(1, 16);
        let editor = ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(rows).desired_width(f32::INFINITY));
        if editor.changed() {
            self.docs.edited_at = now;
            self.docs.edited.insert(key, text.clone());
        }
        let request = hash_of(&text);
        let waited = now - self.docs.edited_at;
        match self.docs.renders.get(&request) {
            Some(Ok((texture, size))) => {
                let width = (size[0] as f32 / 2.0).min(ui.available_width());
                let height = width * size[1] as f32 / size[0].max(1) as f32;
                ui.add(egui::Image::new((texture.id(), vec2(width, height))));
            }
            Some(Err(message)) => {
                ui.add(egui::Label::new(RichText::new(message).small().color(t.danger)).wrap());
            }
            None => {
                ui.colored_label(t.text_dim, tx("compiling-the-example"));
                if waited < SETTLE {
                    ctx.request_repaint_after(Duration::from_secs_f64(SETTLE - waited));
                } else if self.docs.asked.insert(request) {
                    self.compiler.send(Job::Example { request, text: text.clone() });
                }
            }
        }
        let edited = text != original;
        let (mut insert, mut restore) = (false, false);
        ui.horizontal(|ui| {
            insert = ui.small_button(tx("insert-at-the-caret")).clicked();
            if edited {
                restore = ui.small_button(tx("restore-the-example")).clicked();
            }
        });
        if insert {
            self.docs_insert(ctx, &text);
        }
        if restore {
            self.docs.edited.remove(&key);
        }
    }

    fn docs_insert(&mut self, ctx: &egui::Context, code: &str) {
        let block = format!("{}\n", code.trim_end());
        self.edit(ctx, |text, sel| edit::insert_block(text, sel, &block));
    }

    /// F1: the documentation, searching for the word at the caret.
    pub(super) fn help_at_caret(&mut self, ctx: &egui::Context) {
        let at = self.selection(ctx).0;
        let is_word = |c: &char| c.is_alphanumeric() || *c == '_';
        let before: Vec<char> = self.text.chars().take(at).collect();
        let start: String = before.iter().rev().take_while(|c| is_word(c)).collect::<Vec<_>>().into_iter().rev().collect();
        let end: String = self.text.chars().skip(at).take_while(is_word).collect();
        self.docs.search = format!("{start}{end}");
        self.side = Some(Side::Docs);
    }

    /// Opens a lesson, as its button does.
    pub fn open_lesson(&mut self, lesson: usize) {
        self.docs.open(Page::Lesson(lesson));
    }

    /// Searches the documentation, as its field does.
    pub fn search_docs(&mut self, query: &str) {
        query.clone_into(&mut self.docs.search);
    }

    /// The documentation's search, for tests.
    pub fn docs_search(&self) -> &str {
        &self.docs.search
    }
}
