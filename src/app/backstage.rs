//! The File view: a full-window page in place of everything else until Esc or the back arrow,
//! in the layout of MK Matrix's Backstage (MIT OR Apache-2.0; see NOTICE).

use crate::i18n::tx;
use egui::{Align2, Color32, Key, Rect, RichText, Sense, Ui, pos2, vec2};

use super::{App, Pending};
use crate::icons::{self, Icon};
use crate::recent;
use crate::splash;
use crate::templates::TEMPLATES;
use crate::theme::{self, Tokens};

const NAV_W: f32 = 196.0;
// Five templates side by side.
const CONTENT_W: f32 = 860.0;
const MODIFIED_W: f32 = 150.0;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    Home,
    New,
    Open,
    Info,
    SaveAs,
    Export,
    Options,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Page::Home => tx("home"),
            Page::New => tx("new"),
            Page::Open => tx("open-2"),
            Page::Info => tx("info"),
            Page::SaveAs => tx("save-as-2"),
            Page::Export => tx("export"),
            Page::Options => tx("options"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Home => Icon::Home,
            Page::New => Icon::NewFile,
            Page::Open => Icon::OpenFolder,
            Page::Info => Icon::Info,
            Page::SaveAs => Icon::Save,
            Page::Export => Icon::Share,
            Page::Options => Icon::Settings,
        }
    }
}

fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let sel = Color32::from_white_alpha(26);
    let (rect, resp) = ui.allocate_exact_size(vec2(NAV_W, 38.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, sel);
        p.rect_filled(Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 0.0, splash::BRAND);
    } else if resp.hovered() {
        p.rect_filled(rect, 0.0, sel.gamma_multiply(0.55));
    }
    icons::paint_mono(p, Rect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(17.0, 17.0)), icon, Color32::WHITE);
    p.text(pos2(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, label, theme::ui_font(14.0), Color32::WHITE);
    resp
}

fn section(ui: &mut Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(RichText::new(title).font(theme::ui_bold(15.0)).color(t.text));
    ui.add_space(8.0);
}

fn dim(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(text).font(theme::ui_font(12.5)).color(t.text_dim));
}

/// A new-document tile: a small page drawn in code over its name. `kind` varies the lines.
fn tile(ui: &mut Ui, label: &str, kind: Option<usize>) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(158.0, 146.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let p = ui.painter();
    let thumb = Rect::from_min_size(rect.min, vec2(rect.width(), 112.0));
    p.rect_filled(thumb, theme::RADIUS, if resp.hovered() { t.hover } else { t.desk });
    let page = Rect::from_center_size(thumb.center(), vec2(72.0, 96.0));
    theme::paint_sheet_shadow(p, page, &t);
    p.rect_filled(page, 0.0, Color32::WHITE);
    if resp.hovered() {
        p.rect_stroke(page, 0.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Outside);
    }
    if let Some(i) = kind {
        let inner = page.shrink(8.0);
        let ink = Color32::from_gray(150);
        // A title in the accent, and lines of text whose lengths differ from one template to the next.
        p.rect_filled(Rect::from_center_size(pos2(inner.center().x, inner.top() + 3.0), vec2(30.0, 3.0)), 0.0, t.accent);
        for r in 0..8_usize {
            let share = 0.55 + ((r * (i + 3) * 7 + i * 5) % 10) as f32 / 22.0;
            let y = inner.top() + 12.0 + r as f32 * 8.0;
            p.rect_filled(Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width() * share.min(1.0), 2.0)), 0.0, ink);
        }
    }
    let galley = p.layout(label.to_string(), theme::ui_font(12.5), t.text, rect.width());
    p.galley(pos2(rect.left(), thumb.bottom() + 6.0), galley, t.text);
    resp
}

/// `text` with its middle replaced by an ellipsis where it's too wide: both ends of a path say the most.
fn shortened_in_the_middle(text: &str, room: f32, width: impl Fn(&str) -> f32) -> String {
    if width(text) <= room {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let keeping = |kept: usize| {
        let tail = kept / 2;
        let head = kept - tail;
        let start = chars.iter().take(head).collect::<String>();
        let end = chars.iter().skip(chars.len().saturating_sub(tail)).collect::<String>();
        format!("{start}…{end}")
    };
    // The most characters that fit, found by halving: a path can be any length.
    let (mut fits, mut too_many) = (0, chars.len());
    while fits + 1 < too_many {
        let middle = fits + (too_many - fits) / 2;
        if width(&keeping(middle)) <= room {
            fits = middle;
        } else {
            too_many = middle;
        }
    }
    keeping(fits)
}

fn action_row(ui: &mut Ui, icon: Icon, title: &str, detail: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().min(CONTENT_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 56.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 4.0, t.hover);
    }
    icons::paint_mono(p, Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), vec2(24.0, 24.0)), icon, t.text);
    p.text(pos2(rect.left() + 52.0, rect.center().y - 9.0), Align2::LEFT_CENTER, title, theme::ui_bold(13.5), t.text);
    p.text(pos2(rect.left() + 52.0, rect.center().y + 10.0), Align2::LEFT_CENTER, detail, theme::ui_font(12.0), t.text_dim);
    resp
}

fn square_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 88.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let p = ui.painter();
    p.rect_filled(rect, 6.0, if resp.hovered() { t.hover } else { t.ribbon });
    icons::paint_mono(p, Rect::from_center_size(pos2(rect.center().x, rect.top() + 32.0), vec2(30.0, 30.0)), icon, t.text);
    let galley = p.layout(label.to_string(), theme::ui_font(12.0), t.text, rect.width() - 8.0);
    p.galley(pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 56.0), galley, t.text);
    resp
}

impl App {
    pub(super) fn open_backstage(&mut self, page: Page) {
        self.backstage = Some(page);
        self.find = None;
        self.ask_modified();
    }

    /// Asks when the recent documents last changed, on a thread of its own: a folder on a slow disk
    /// or a network mount must not hold up drawing.
    fn ask_modified(&self) {
        let paths = self.recent.clone();
        let found = std::sync::Arc::clone(&self.recent_ages);
        std::thread::spawn(move || {
            for path in paths {
                let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).map(|e| e.as_secs());
                if let Ok(mut found) = found.lock() {
                    found.insert(path, age);
                }
            }
        });
    }

    pub(super) fn backstage_ui(&mut self, ui: &mut Ui) {
        let Some(mut page) = self.backstage else { return };
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let mut close = false;
        egui::Panel::left("backstage_nav")
            .exact_size(NAV_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(splash::INK).inner_margin(egui::Margin { left: 0, right: 0, top: 10, bottom: 10 }))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                close |= nav_item(ui, Icon::Back, tx("back"), false).clicked();
                let (rect, _) = ui.allocate_exact_size(vec2(NAV_W, 58.0), Sense::hover());
                splash::pearl(ui.painter(), pos2(rect.left() + 32.0, rect.center().y), 13.0);
                ui.painter().text(pos2(rect.left() + 54.0, rect.center().y), Align2::LEFT_CENTER, "Pearl", theme::ui_font(18.0), Color32::WHITE);
                for item in [Page::Home, Page::New, Page::Open] {
                    if nav_item(ui, item.icon(), item.label(), page == item).clicked() {
                        page = item;
                    }
                }
                ui.add_space(17.0);
                if nav_item(ui, Page::Info.icon(), Page::Info.label(), page == Page::Info).clicked() {
                    page = Page::Info;
                }
                if nav_item(ui, Icon::Clock, tx("version-history"), false).clicked() {
                    self.side = Some(super::Side::Versions);
                    close = true;
                }
                if nav_item(ui, Icon::Save, tx("save"), false).clicked() {
                    if self.path.is_some() {
                        close |= self.save(false);
                    } else {
                        page = Page::SaveAs;
                    }
                }
                for item in [Page::SaveAs, Page::Export] {
                    if nav_item(ui, item.icon(), item.label(), page == item).clicked() {
                        page = item;
                    }
                }
                if nav_item(ui, Icon::Close, tx("close"), false).on_hover_text(tx("closes-the-document-and-starts-a-blank")).clicked() {
                    self.request(&ctx, Pending::New);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if nav_item(ui, Page::Options.icon(), Page::Options.label(), page == Page::Options).clicked() {
                        page = Page::Options;
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.ribbon)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // In the middle of a wide window, and against the margins of a narrow one.
                let left = ((ui.available_width() - CONTENT_W) / 2.0).max(44.0);
                ui.add_space(26.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(left);
                    ui.vertical(|ui| {
                        ui.set_max_width(CONTENT_W.min(ui.available_width() - 32.0));
                        ui.label(RichText::new(page.label()).font(theme::ui_font(28.0)).color(t.text));
                        ui.add_space(18.0);
                        match page {
                            Page::Home => {
                                section(ui, tx("new"));
                                self.new_tiles(ui, &ctx);
                                ui.add_space(18.0);
                                section(ui, tx("recent"));
                                self.recent_list(ui, &ctx, 8);
                            }
                            Page::New => self.new_tiles(ui, &ctx),
                            Page::Open => {
                                ui.horizontal(|ui| {
                                    if square_button(ui, Icon::OpenFolder, tx("browse")).clicked() {
                                        self.request(&ctx, Pending::Open);
                                    }
                                    if square_button(ui, Icon::Folder, tx("open-folder")).clicked() {
                                        self.request(&ctx, Pending::OpenFolder);
                                    }
                                });
                                ui.add_space(18.0);
                                section(ui, tx("recent"));
                                self.recent_list(ui, &ctx, usize::MAX);
                            }
                            Page::Info => self.pane_details(ui, &t),
                            Page::SaveAs => {
                                dim(ui, tx("images-and-bibliographies-are-read-from-the"));
                                ui.add_space(12.0);
                                if square_button(ui, Icon::Folder, tx("browse")).clicked() && self.save(true) {
                                    close = true;
                                }
                            }
                            Page::Export => {
                                if action_row(ui, Icon::Print, tx("export-as-pdf-2"), tx("the-pages-ready-to-print-or-send")).clicked() {
                                    self.export_pdf();
                                }
                                if action_row(ui, Icon::Print, tx("export-accessible-pdf"), tx("export-accessible-pdf-detail")).clicked() {
                                    self.export_accessible_pdf();
                                }
                                if action_row(ui, Icon::Save, tx("save-a-copy"), tx("the-typst-source-in-another-file-without")).clicked() {
                                    self.save_copy();
                                }
                            }
                            Page::Options => self.options(ui, &ctx),
                        }
                    });
                });
                ui.add_space(32.0);
            });
        });
        let free = self.confirm.is_none() && !egui::Popup::is_any_open(&ctx);
        if free && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
        }
        // An action on a page may already have closed the view (a template, a recent document).
        if self.backstage.is_some() {
            self.backstage = Some(page);
        }
        if close {
            self.backstage = None;
            self.want_focus = true;
        }
    }

    fn new_tiles(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(17.0, 14.0);
            if tile(ui, tx("blank-document"), None).clicked() {
                self.request(ctx, Pending::New);
            }
            for (i, template) in TEMPLATES.iter().enumerate() {
                if tile(ui, tx(template.name), Some(i)).clicked() {
                    self.request(ctx, Pending::Template(i));
                }
            }
        });
    }

    fn recent_list(&mut self, ui: &mut Ui, ctx: &egui::Context, limit: usize) {
        let t = Tokens::get(ctx);
        if self.recent.is_empty() {
            dim(ui, tx("no-recent-documents-yet"));
            return;
        }
        let width = ui.available_width().min(CONTENT_W);
        let (head, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
        ui.painter().text(pos2(head.right() - MODIFIED_W, head.center().y), Align2::LEFT_CENTER, tx("modified"), theme::ui_font(11.5), t.text_dim);
        let ages = self.recent_ages.lock().map(|a| a.clone()).unwrap_or_default();
        let mut chosen = None;
        for path in self.recent.iter().take(limit) {
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
            let folder = path.parent().map_or_else(String::new, |p| p.display().to_string());
            let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, t.hover);
            }
            icons::paint_mono(
                ui.painter(),
                Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(20.0, 20.0)),
                // A folder opened as a project is kept on the list too.
                if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("typ")) { Icon::FileName } else { Icon::Folder },
                t.text,
            );
            let text_right = rect.right() - MODIFIED_W - 8.0;
            let clip = ui.painter().with_clip_rect(Rect::from_min_max(rect.min, pos2(text_right, rect.bottom())));
            let room = text_right - rect.left() - 44.0;
            let measure = |text: &str, font: egui::FontId| ui.painter().layout_no_wrap(text.to_string(), font, t.text).size().x;
            let shown_name = shortened_in_the_middle(&name, room, |s| measure(s, theme::ui_bold(13.0)));
            let shown_folder = shortened_in_the_middle(&folder, room, |s| measure(s, theme::ui_font(11.5)));
            clip.text(pos2(rect.left() + 44.0, rect.center().y - 9.0), Align2::LEFT_CENTER, shown_name, theme::ui_bold(13.0), t.text);
            clip.text(pos2(rect.left() + 44.0, rect.center().y + 9.0), Align2::LEFT_CENTER, shown_folder, theme::ui_font(11.5), t.text_dim);
            let when = match ages.get(path) {
                Some(Some(seconds)) => recent::age(*seconds),
                Some(None) => tx("not-found").to_string(),
                None => String::new(),
            };
            ui.painter().text(pos2(rect.right() - MODIFIED_W, rect.center().y), Align2::LEFT_CENTER, when, theme::ui_font(12.0), t.text_dim);
            if resp.on_hover_text(path.display().to_string()).clicked() {
                chosen = Some(path.clone());
            }
        }
        if let Some(path) = chosen {
            self.request(ctx, Pending::Recent(path));
        }
    }

    fn options(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        section(ui, tx("saving"));
        ui.checkbox(&mut self.autosave, tx("autosave-2"));
        dim(ui, tx("writes-to-the-file-about-a-second"));
        ui.add_space(14.0);
        section(ui, tx("appearance"));
        for choice in theme::ThemeChoice::ALL {
            if ui.radio(self.theme == choice, choice.label()).clicked() {
                self.choose_theme(ctx, choice);
            }
        }
        ui.add_space(14.0);
        section(ui, tx("language"));
        for lang in crate::i18n::Lang::ALL {
            if ui.radio(crate::i18n::current() == lang, lang.name()).clicked() {
                crate::i18n::choose(lang);
            }
        }
        section(ui, tx("editor"));
        ui.checkbox(&mut self.line_numbers, tx("show-line-numbers"));
    }
}

#[cfg(test)]
mod tests {
    use super::shortened_in_the_middle;

    fn in_chars(text: &str) -> f32 {
        text.chars().count() as f32
    }

    #[test]
    fn what_fits_is_left_alone() {
        assert_eq!(shortened_in_the_middle("/home/a", 7.0, in_chars), "/home/a");
        assert_eq!(shortened_in_the_middle("", 0.0, in_chars), "");
    }

    #[test]
    fn a_long_path_keeps_both_ends_and_fills_the_room() {
        let short = shortened_in_the_middle("/home/alguém/Documentos/trabalho/final", 15.0, in_chars);
        assert_eq!(short, "/home/a…o/final");
    }

    #[test]
    fn no_room_at_all_is_only_the_ellipsis() {
        assert_eq!(shortened_in_the_middle("/um/caminho", 0.0, in_chars), "…");
    }
}
