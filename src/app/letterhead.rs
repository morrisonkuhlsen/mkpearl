//! The dialog for a letterhead and a watermark, with a preview that follows it.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui, vec2};

use super::App;
use super::docs::hash_of;
use crate::compiler::Job;
use crate::i18n::tx;
use crate::images;
use crate::letterhead::{self, ANGLES, Letterhead, OPACITIES, SHADES, SIZES};
use crate::recent;
use crate::theme::Tokens;

/// The last one used, with its pictures' full paths, to start from in another document.
const MODEL_FILE: &str = "letterhead.json";
const PREVIEW_WIDTH: f32 = 280.0;

pub(super) struct LetterheadDialog {
    settings: Letterhead,
    /// The document already has one, which can be taken out.
    existing: bool,
    last_preview: Option<u64>,
}

impl App {
    /// Opens the dialog on what the document has, or else on the last one used.
    pub fn open_letterhead(&mut self) {
        let existing = letterhead::in_document(&self.text);
        let model = || {
            let text = recent::read_setting(MODEL_FILE)?;
            Some(Letterhead::from_json(&serde_json::from_str(&text).ok()?))
        };
        let is_new = existing.is_none();
        let mut settings = existing.or_else(model).unwrap_or_default();
        if settings.mark_text.is_empty() {
            tx("draft").clone_into(&mut settings.mark_text);
        }
        if is_new {
            // What's turned on is the writer's choice in each document, but a head only comes with its text.
            settings.head_on = !settings.name.is_empty();
        }
        self.letterhead_dialog = Some(LetterheadDialog { settings, existing: !is_new, last_preview: None });
    }

    /// Changes the settings in the open dialog, as its fields do.
    pub fn edit_letterhead(&mut self, change: impl FnOnce(&mut Letterhead)) {
        if let Some(dialog) = &mut self.letterhead_dialog {
            change(&mut dialog.settings);
        }
    }

    pub(super) fn letterhead_window(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(mut dialog) = self.letterhead_dialog.take() else { return };
        let (mut apply, mut remove, mut close) = (false, false, false);
        egui::Window::new(tx("letterhead-and-watermark")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(330.0);
                        Self::letterhead_form(ui, &mut dialog.settings);
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            apply = ui.button(tx("apply")).clicked();
                            if dialog.existing {
                                remove = ui.button(tx("remove")).clicked();
                            }
                            close = ui.button(tx("cancel")).clicked();
                        });
                    });
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width(PREVIEW_WIDTH);
                        self.letterhead_preview(ui, &mut dialog, t);
                    });
                });
            },
        );
        if apply {
            close = self.apply_letterhead(ctx, &dialog.settings);
        }
        if remove {
            self.edit(ctx, |text, sel| letterhead::apply(text, sel, &Letterhead::default()));
            close = true;
        }
        if !close {
            self.letterhead_dialog = Some(dialog);
        }
    }

    fn letterhead_form(ui: &mut Ui, s: &mut Letterhead) {
        let field = |ui: &mut Ui, label: &str, value: &mut String| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
        };
        let picture = |ui: &mut Ui, label: &str, value: &mut String| {
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.button(tx("choose")).clicked()
                    && let Some(file) = rfd::FileDialog::new().add_filter(tx("images"), &images::EXTENSIONS).pick_file()
                {
                    *value = file.to_string_lossy().into_owned();
                }
                if !value.is_empty() && ui.button(tx("remove")).clicked() {
                    value.clear();
                }
            });
            if !value.is_empty() {
                let name = Path::new(value.as_str()).file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                ui.add(egui::Label::new(RichText::new(name).small()).truncate());
            }
        };
        ui.checkbox(&mut s.head_on, RichText::new(tx("letterhead")).strong());
        if s.head_on {
            field(ui, tx("name-or-company"), &mut s.name);
            field(ui, tx("address-or-details"), &mut s.details);
            field(ui, tx("footer-text"), &mut s.footer);
            picture(ui, tx("logo"), &mut s.logo);
            ui.checkbox(&mut s.first_page_only, tx("only-on-the-first-page"));
        }
        ui.add_space(8.0);
        ui.checkbox(&mut s.mark_on, RichText::new(tx("watermark")).strong());
        if s.mark_on {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.mark_is_image, false, tx("text"));
                ui.selectable_value(&mut s.mark_is_image, true, tx("image"));
            });
            if s.mark_is_image {
                picture(ui, tx("image"), &mut s.mark_image);
                ui.add(egui::Slider::new(&mut s.mark_opacity, OPACITIES).text(tx("opacity")).suffix(" %"));
            } else {
                field(ui, tx("text"), &mut s.mark_text);
                ui.add(egui::Slider::new(&mut s.mark_size, SIZES).text(tx("size")).suffix(" pt"));
                ui.add(egui::Slider::new(&mut s.mark_shade, SHADES).text(tx("lightness")));
            }
            ui.add(egui::Slider::new(&mut s.mark_angle, ANGLES).text(tx("angle")).suffix("°"));
        }
    }

    fn letterhead_preview(&mut self, ui: &mut Ui, dialog: &mut LetterheadDialog, t: &Tokens) {
        let code = dialog.settings.preview(tx("your-text-goes-here"));
        let request = hash_of(&code);
        // Pictures of whole pages are large: only the one on show is kept.
        if let Some(old) = dialog.last_preview
            && old != request
            && self.docs.renders.contains_key(&request)
        {
            self.docs.renders.remove(&old);
            self.docs.asked.remove(&old);
            dialog.last_preview = Some(request);
        }
        if dialog.last_preview.is_none() {
            dialog.last_preview = Some(request);
        }
        if !self.docs.renders.contains_key(&request) && self.docs.asked.insert(request) {
            self.compiler.send(Job::Example { request, text: code });
        }
        let shown = self.docs.renders.get(&request).or_else(|| dialog.last_preview.and_then(|old| self.docs.renders.get(&old)));
        match shown {
            Some(Ok((texture, size))) => {
                let height = PREVIEW_WIDTH * size[1] as f32 / size[0].max(1) as f32;
                ui.add(egui::Image::new((texture.id(), vec2(PREVIEW_WIDTH, height))));
            }
            Some(Err(message)) => {
                ui.add(egui::Label::new(RichText::new(message).small().color(t.danger)).wrap());
            }
            None => {
                ui.colored_label(t.text_dim, tx("compiling-the-example"));
            }
        }
    }

    /// Brings the pictures into the document's folder and writes the block. Whether it went well.
    pub fn apply_letterhead_now(&mut self, ctx: &egui::Context) -> bool {
        let Some(dialog) = self.letterhead_dialog.take() else { return false };
        let done = self.apply_letterhead(ctx, &dialog.settings);
        if !done {
            self.letterhead_dialog = Some(dialog);
        }
        done
    }

    fn apply_letterhead(&mut self, ctx: &egui::Context, settings: &Letterhead) -> bool {
        let mut written = settings.clone();
        let mut model = settings.clone();
        let wants_logo = settings.head_on && !settings.logo.is_empty();
        let wants_mark = settings.mark_on && settings.mark_is_image && !settings.mark_image.is_empty();
        if wants_logo || wants_mark {
            let Some(dir) = self.images_folder() else { return false };
            let mut bring = |path: &str, into: &mut String, full: &mut String| -> bool {
                let file = if Path::new(path).is_absolute() { PathBuf::from(path) } else { dir.join(path) };
                match images::adopt(&dir, &file) {
                    Ok(inside) => {
                        *full = file.to_string_lossy().into_owned();
                        *into = inside;
                        true
                    }
                    Err(id) => {
                        self.status = tx(id).into();
                        false
                    }
                }
            };
            if wants_logo && !bring(&settings.logo, &mut written.logo, &mut model.logo) {
                return false;
            }
            if wants_mark && !bring(&settings.mark_image, &mut written.mark_image, &mut model.mark_image) {
                return false;
            }
        }
        recent::write_setting(MODEL_FILE, &model.to_json().to_string());
        self.edit(ctx, |text, sel| letterhead::apply(text, sel, &written));
        true
    }
}
