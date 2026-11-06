//! The terminal pane, beside the rail: a program run in the document's folder, such as Claude Code
//! or Codex, while the pages follow what it writes (see `watch`). It starts only from a click here.

use std::path::PathBuf;

use egui::{Align2, Color32, CursorIcon, Event, FontId, Id, Key, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::App;
use crate::i18n::tx;
use crate::icons::Icon;
use crate::terminal::{Caret, Drawn, Launch, Palette, Program, Terminal};
use crate::theme::{self, Tokens};
use crate::tr;
use crate::widgets::icon_button_mono;

/// The pane's narrowest, and where it starts.
pub(super) const MIN_WIDTH: f32 = 320.0;
const START_WIDTH: f32 = 600.0;
const FONT_SIZE: f32 = 13.0;
const HEADER: f32 = 34.0;
const PADDING: f32 = 6.0;

fn grid_id() -> Id {
    Id::unique("pearl-terminal-grid")
}

pub(super) struct TerminalPane {
    pub(super) open: bool,
    pub(super) width: f32,
    running: Option<Terminal>,
    /// Why the last start failed.
    problem: Option<String>,
    /// Wheel movement not yet a whole line.
    scroll_rest: f32,
    /// The cell the pointer went down on, while selecting.
    selecting: bool,
}

impl Default for TerminalPane {
    fn default() -> Self {
        Self { open: false, width: START_WIDTH, running: None, problem: None, scroll_rest: 0.0, selecting: false }
    }
}

impl TerminalPane {
    pub(super) fn is_running(&self) -> bool {
        self.running.as_ref().is_some_and(|t| t.ended.is_none())
    }
}

fn font() -> FontId {
    FontId::monospace(FONT_SIZE)
}

fn palette(t: &Tokens, ctx: &egui::Context) -> Palette {
    let dark = ctx.style_of(ctx.theme()).visuals.dark_mode;
    Palette::new(dark, t.text, t.input_bg, t.accent)
}

impl App {
    /// Whether the terminal has the keyboard.
    pub fn terminal_focused(&self, ctx: &egui::Context) -> bool {
        self.terminal.open && self.terminal.running.is_some() && ctx.memory(|m| m.has_focus(grid_id()))
    }

    /// Opens or hides the pane. A program running in it goes on running.
    pub fn toggle_terminal(&mut self) {
        self.terminal.open = !self.terminal.open;
    }

    pub fn terminal_open(&self) -> bool {
        self.terminal.open
    }

    /// The text on the terminal's screen, as tests read it.
    pub fn terminal_text(&self, ctx: &egui::Context) -> Option<String> {
        let t = Tokens::get(ctx);
        self.terminal.running.as_ref().map(|term| term.text(&palette(&t, ctx)))
    }

    /// Where programs start: the document's folder, or the home folder for a document with no file.
    fn terminal_folder(&self) -> PathBuf {
        if let Some(root) = self.project_folder() {
            return root.to_path_buf();
        }
        let parent = self.path.as_deref().and_then(std::path::Path::parent).filter(|p| !p.as_os_str().is_empty()).map(PathBuf::from);
        let home = || std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        let folder = parent.or_else(home).unwrap_or_else(|| PathBuf::from("."));
        std::path::absolute(&folder).unwrap_or(folder)
    }

    /// Starts `program` in the document's folder, ending what ran before. Only a click calls this.
    /// An agent needs a file to write: a document with none is saved first, or nothing starts.
    pub fn start_terminal(&mut self, ctx: &egui::Context, program: Program) {
        self.terminal.problem = None;
        if program != Program::Shell && self.path.is_none() && !self.save(true) {
            self.terminal.problem = Some(tx("save-the-document-before-the-agent").to_string());
            return;
        }
        self.terminal.running = None;
        let folder = self.terminal_folder();
        let missing = tr!("terminal-program-missing", program = program.name());
        let (cw, ch) = cell_size(ctx);
        let cell = (cw.round() as u16, ch.round() as u16);
        let document = self.path.as_deref().map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
        let launch = Launch { program, command: program.command(), folder: &folder, document: document.as_deref(), missing: &missing };
        match Terminal::start(ctx, &launch, (80, 24), cell) {
            Ok(term) => {
                self.terminal.running = Some(term);
                self.terminal.open = true;
                ctx.memory_mut(|m| m.request_focus(grid_id()));
            }
            Err(e) => self.terminal.problem = Some(tr!("cant-start-the-terminal", error = e.to_string())),
        }
    }

    /// Sends text to the program as if typed, for tests and the screenshots.
    pub fn type_in_terminal(&self, text: &str) {
        if let Some(term) = &self.terminal.running {
            term.input(text.as_bytes().to_vec());
        }
    }

    /// Ends the program, and the terminal with it.
    pub fn end_terminal(&mut self) {
        self.terminal.running = None;
    }

    /// Takes the keyboard's events for the terminal, before anything else in the window sees them.
    pub(super) fn terminal_keys(&mut self, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        if let Some(term) = &mut self.terminal.running {
            term.poll(ctx, &palette(&t, ctx));
        }
        let focused = self.terminal_focused(ctx);
        let Some(term) = &mut self.terminal.running else { return };
        term.set_focused(focused);
        if !focused || term.ended.is_some() {
            return;
        }
        let mode = term.mode();
        let events = ctx.input_mut(|i| {
            let (mine, rest): (Vec<Event>, Vec<Event>) = std::mem::take(&mut i.events).into_iter().partition(|e| {
                matches!(e, Event::Key { .. } | Event::Text(_) | Event::Copy | Event::Cut | Event::Paste(_) | Event::PasteImage(_) | Event::Ime(_))
            });
            i.events = rest;
            mine
        });
        let mac = cfg!(target_os = "macos");
        let alt_held = ctx.input(|i| i.modifiers.alt && !i.modifiers.ctrl);
        for event in events {
            match event {
                Event::Key { key, pressed: true, modifiers, .. } => {
                    // Copying is Ctrl+Shift+C, as in other terminals: Ctrl+C belongs to the program.
                    if !mac && key == Key::C && modifiers.ctrl && modifiers.shift {
                        if let Some(text) = term.selected_text() {
                            ctx.copy_text(text);
                        }
                        continue;
                    }
                    if let Some(bytes) = crate::terminal::key_bytes(key, modifiers, mode) {
                        term.input(bytes);
                    }
                }
                // Alt with a letter came as a key already; on macOS, Option makes characters.
                Event::Text(text) if !(alt_held && !mac) => term.input(text.into_bytes()),
                Event::Ime(egui::ImeEvent::Commit(text)) => term.input(text.into_bytes()),
                Event::Copy => match term.selected_text() {
                    Some(text) => {
                        ctx.copy_text(text);
                        term.clear_selection();
                    }
                    None if !mac => term.input(vec![0x03]),
                    None => {}
                },
                Event::Cut if !mac => term.input(vec![0x18]),
                Event::Paste(text) => term.paste(&text),
                // An image is the program's to read from the clipboard, as Ctrl+V asks it to.
                Event::PasteImage(_) => term.input(vec![0x16]),
                _ => {}
            }
        }
    }

    /// The pane: its header and the screen, or what can be started.
    pub(super) fn terminal_pane(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(rect, 0.0, t.ribbon);
        ui.painter().vline(rect.right() - 1.0, rect.y_range(), Stroke::new(1.0, t.separator));
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER));
        let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), pos2(rect.right() - 1.0, rect.bottom()));
        self.terminal_header(ui, header.shrink2(vec2(8.0, 4.0)), t);
        // The right edge moves the pane's width.
        let edge = Rect::from_min_max(pos2(rect.right() - 4.0, rect.top()), pos2(rect.right() + 2.0, rect.bottom()));
        let drag = ui.interact(edge, ui.scope_id().with("terminal-edge"), Sense::drag());
        if drag.hovered() || drag.dragged() {
            ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if drag.dragged() {
            self.terminal.width = (self.terminal.width + drag.drag_delta().x).max(MIN_WIDTH);
        }
        if self.terminal.running.is_some() {
            self.terminal_screen(ui, body, t);
        } else {
            self.terminal_start(ui, body, t);
        }
    }

    fn terminal_header(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let title = match &self.terminal.running {
            Some(term) => term.title.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| term.program.name().to_string()),
            None => tx("terminal").to_string(),
        };
        // The buttons first, from the right, so a long title is cut instead of running under them.
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            if icon_button_mono(ui, Icon::Left, t.text_dim, tx("hide-the-terminal"), vec2(24.0, 22.0)).clicked() {
                self.terminal.open = false;
            }
            if self.terminal.running.is_some() && icon_button_mono(ui, Icon::Delete, t.text_dim, tx("end-the-program"), vec2(24.0, 22.0)).clicked() {
                self.end_terminal();
            }
            if self.terminal.running.as_ref().is_some_and(|term| term.ended.is_some()) {
                for program in [Program::Shell, Program::Codex, Program::Claude] {
                    if ui.small_button(program.name()).on_hover_text(tr!("start-in-the-documents-folder", program = program.name())).clicked() {
                        self.start_terminal(&ctx, program);
                    }
                }
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                crate::icons::paint(ui.painter(), r, Icon::Terminal, t.text);
                ui.add(egui::Label::new(egui::RichText::new(title).font(theme::ui_bold(13.0)).color(t.text)).truncate());
            });
        });
    }

    /// With nothing running: what can be started, and where.
    fn terminal_start(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let folder = self.terminal_folder();
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(14.0)), |ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(tx("run-a-program-beside-the-document")).font(theme::ui_bold(13.0)).color(t.text));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr!("terminal-starts-in", folder = folder.display().to_string())).color(t.text_dim));
            ui.add_space(10.0);
            for program in [Program::Claude, Program::Codex, Program::Shell] {
                let button = egui::Button::new(egui::RichText::new(program.name()).font(theme::ui_font(13.0))).min_size(vec2(160.0, 28.0));
                if ui.add(button).on_hover_text(tr!("start-in-the-documents-folder", program = program.name())).clicked() {
                    self.start_terminal(&ctx, program);
                }
                ui.add_space(4.0);
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new(tx("terminal-programs-are-your-own")).font(theme::ui_font(12.0)).color(t.text_dim));
            if let Some(problem) = &self.terminal.problem {
                ui.add_space(8.0);
                ui.colored_label(t.danger, problem);
            }
        });
    }

    fn terminal_screen(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let pal = palette(t, &ctx);
        ui.painter().rect_filled(rect, 0.0, pal.background());
        let (cw, ch) = cell_size(&ctx);
        let grid = rect.shrink(PADDING);
        let columns = ((grid.width() / cw).floor() as usize).max(2);
        let lines = ((grid.height() / ch).floor() as usize).max(1);
        let ended = self.terminal.running.as_ref().and_then(|term| term.ended);
        let Some(term) = &mut self.terminal.running else { return };
        term.resize(columns, lines, (cw.round() as u16, ch.round() as u16));

        let id = grid_id();
        let resp = ui.interact(rect, id, Sense::click_and_drag());
        if resp.clicked() || resp.drag_started() {
            resp.request_focus();
        }
        let focused = resp.has_focus();
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
            });
        }
        let cell_at = |p: Pos2| -> ((usize, usize), bool) {
            let x = ((p.x - grid.left()) / cw).max(0.0);
            let y = ((p.y - grid.top()) / ch).max(0.0);
            let column = (x.floor() as usize).min(columns - 1);
            ((column, (y.floor() as usize).min(lines - 1)), x.fract() >= 0.5)
        };

        // The pointer: selecting, opening links with Ctrl, and the wheel.
        let hovered_link = resp.hover_pos().and_then(|p| term.link_at(cell_at(p).0));
        let command = ui.input(|i| i.modifiers.command);
        if let Some(link) = &hovered_link {
            if command {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            resp.clone().on_hover_text(tr!("ctrl-click-to-open", url = link.clone()));
        } else if resp.hovered() {
            ctx.set_cursor_icon(CursorIcon::Text);
        }
        if resp.clicked()
            && command
            && let Some(link) = hovered_link.clone()
        {
            ctx.open_url(egui::OpenUrl::new_tab(link));
        } else if let Some(p) = resp.interact_pointer_pos() {
            let (at, right) = cell_at(p);
            if resp.drag_started() {
                term.select_from(at, right, 1);
                self.terminal.selecting = true;
            } else if resp.dragged() && self.terminal.selecting {
                term.select_to(at, right);
            } else if resp.triple_clicked() {
                term.select_from(at, right, 3);
            } else if resp.double_clicked() {
                term.select_from(at, right, 2);
            } else if resp.clicked() {
                term.clear_selection();
            }
        }
        if resp.drag_stopped() {
            self.terminal.selecting = false;
        }
        if resp.hovered() {
            // In lines; positive moves the content down, showing what's above.
            let delta = ui.input(|i| {
                i.events
                    .iter()
                    .map(|e| match e {
                        Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, .. } => delta.y / ch,
                        Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, .. } => delta.y,
                        Event::MouseWheel { unit: egui::MouseWheelUnit::Page, delta, .. } => delta.y * lines as f32,
                        _ => 0.0,
                    })
                    .sum::<f32>()
            });
            if delta != 0.0 {
                self.terminal.scroll_rest += delta;
                let whole = self.terminal.scroll_rest.trunc();
                self.terminal.scroll_rest -= whole;
                let at = resp.hover_pos().map_or((0, 0), |p| cell_at(p).0);
                term.scroll(whole as i32, at);
            }
        }

        let screen = term.snapshot(&pal);
        let painter = ui.painter_at(rect);
        let origin = grid.min;
        let place = |column: usize, line: usize| pos2(origin.x + column as f32 * cw, origin.y + line as f32 * ch);
        for cell in &screen.cells {
            paint_cell(&painter, cell, place(cell.column, cell.line), (cw, ch), hovered_link.is_some());
        }
        if let Some((column, line, shape)) = screen.caret {
            let at = place(column, line);
            let r = Rect::from_min_size(at, vec2(cw, ch));
            match shape {
                Caret::Block => {
                    painter.rect_filled(r, 0.0, t.accent);
                    if let Some(cell) = screen.cells.iter().find(|c| c.line == line && c.column == column && !c.text.is_empty()) {
                        painter.text(at, Align2::LEFT_TOP, &cell.text, font(), pal.background());
                    }
                }
                Caret::Hollow => {
                    painter.rect_stroke(r.shrink(0.5), 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
                }
                Caret::Beam => {
                    painter.rect_filled(Rect::from_min_size(at, vec2(2.0, ch)), 0.0, t.accent);
                }
                Caret::Underline => {
                    painter.rect_filled(Rect::from_min_size(pos2(at.x, at.y + ch - 2.0), vec2(cw, 2.0)), 0.0, t.accent);
                }
            }
        }
        if screen.scrolled > 0 {
            let note = tr!("terminal-scrolled-back", lines = screen.scrolled.to_string());
            painter.text(pos2(rect.right() - 10.0, rect.top() + 6.0), Align2::RIGHT_TOP, note, theme::ui_font(11.5), t.text_dim);
        }
        if let Some(code) = ended {
            let note = match code {
                Some(code) if code != 0 => tr!("the-program-ended-with", code = code.to_string()),
                _ => tx("the-program-ended").to_string(),
            };
            let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 26.0), rect.max);
            painter.rect_filled(bar, 0.0, t.notice_info_bg);
            painter.text(pos2(bar.left() + 10.0, bar.center().y), Align2::LEFT_CENTER, note, theme::ui_font(12.5), t.text);
        }
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, tx("terminal")));
    }
}

/// A cell's width and height in the terminal's font.
fn cell_size(ctx: &egui::Context) -> (f32, f32) {
    let f = font();
    let (w, h) = ctx.fonts_mut(|fonts| (fonts.glyph_width(&f, 'M'), fonts.row_height(&f)));
    (w.max(4.0), h.max(8.0))
}

fn paint_cell(painter: &egui::Painter, cell: &Drawn, at: Pos2, (cw, ch): (f32, f32), link_hovered: bool) {
    let width = if cell.wide { cw * 2.0 } else { cw };
    let r = Rect::from_min_size(at, vec2(width, ch));
    if let Some(bg) = cell.bg {
        painter.rect_filled(r, 0.0, bg);
    }
    if let Some(c) = cell.text.chars().next()
        && cell.text.chars().count() == 1
        && box_drawing(painter, c, r, cell.fg)
    {
        return;
    }
    if !cell.text.is_empty() {
        painter.text(at, Align2::LEFT_TOP, &cell.text, font(), cell.fg);
        if cell.bold {
            // The monospace font has no bold: the same letters, a hair to the right.
            painter.text(at + vec2(0.6, 0.0), Align2::LEFT_TOP, &cell.text, font(), cell.fg);
        }
    }
    if cell.underline || (cell.link && link_hovered) {
        painter.hline(r.x_range(), r.bottom() - 1.5, Stroke::new(1.0, cell.fg));
    }
    if cell.strike {
        painter.hline(r.x_range(), r.center().y, Stroke::new(1.0, cell.fg));
    }
}

/// Lines and blocks drawn to the cell's edges, so boxes join up whatever the font has.
fn box_drawing(painter: &egui::Painter, c: char, r: Rect, colour: Color32) -> bool {
    let (l, rt, t, b) = (r.left(), r.right(), r.top(), r.bottom());
    let (x, y) = (r.center().x.round() + 0.5, r.center().y.round() + 0.5);
    let thin = Stroke::new(1.0, colour);
    let heavy = Stroke::new(2.0, colour);
    // Which arms each line has: left, right, up, down.
    let arms = match c {
        '─' | '╌' | '┄' => Some((true, true, false, false, thin)),
        '━' => Some((true, true, false, false, heavy)),
        '│' | '╎' | '┆' => Some((false, false, true, true, thin)),
        '┃' => Some((false, false, true, true, heavy)),
        '┌' => Some((false, true, false, true, thin)),
        '┐' => Some((true, false, false, true, thin)),
        '└' => Some((false, true, true, false, thin)),
        '┘' => Some((true, false, true, false, thin)),
        '├' => Some((false, true, true, true, thin)),
        '┤' => Some((true, false, true, true, thin)),
        '┬' => Some((true, true, false, true, thin)),
        '┴' => Some((true, true, true, false, thin)),
        '┼' => Some((true, true, true, true, thin)),
        _ => None,
    };
    if let Some((left, right, up, down, stroke)) = arms {
        if left {
            painter.hline(l..=x, y, stroke);
        }
        if right {
            painter.hline(x..=rt, y, stroke);
        }
        if up {
            painter.vline(x, t..=y, stroke);
        }
        if down {
            painter.vline(x, y..=b, stroke);
        }
        return true;
    }
    // Rounded corners: an arc from one edge's middle to the other's.
    let corner = match c {
        '╭' => Some((pos2(rt, y), pos2(x, b))),
        '╮' => Some((pos2(l, y), pos2(x, b))),
        '╯' => Some((pos2(l, y), pos2(x, t))),
        '╰' => Some((pos2(rt, y), pos2(x, t))),
        _ => None,
    };
    if let Some((from, to)) = corner {
        let control = pos2(to.x, from.y);
        let points: Vec<Pos2> = (0..=8)
            .map(|i| {
                let s = i as f32 / 8.0;
                let a = from.lerp(control, s);
                let b = control.lerp(to, s);
                a.lerp(b, s)
            })
            .collect();
        painter.add(egui::Shape::line(points, thin));
        return true;
    }
    let block = match c {
        '█' => Some(r),
        '▀' => Some(Rect::from_min_max(r.min, pos2(rt, r.center().y))),
        '▄' => Some(Rect::from_min_max(pos2(l, r.center().y), r.max)),
        '▌' => Some(Rect::from_min_max(r.min, pos2(r.center().x, b))),
        '▐' => Some(Rect::from_min_max(pos2(r.center().x, t), r.max)),
        _ => None,
    };
    if let Some(block) = block {
        painter.rect_filled(block, 0.0, colour);
        return true;
    }
    false
}
