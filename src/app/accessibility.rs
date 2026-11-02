//! Accessibility around exporting: a warning first, that can be passed or turned off and never stops an
//! export, and the reasons an accessible PDF was refused, in plain words and linked to their places.

use std::path::PathBuf;

use egui::{RichText, Sense};

use super::typography::Watch;
use super::{App, Side};
use crate::compiler::ExportProblem;
use crate::i18n::tx;
use crate::recent;
use crate::theme::{self, Tokens};
use crate::tr;

const CHECK_FILE: &str = "export-check.txt";
/// As many points as the warning lists; the pane has them all.
const MOST_LISTED: usize = 6;

pub(super) struct ExportCheck {
    on: bool,
    pub(super) warning: bool,
    pub(super) refused: Option<Vec<ExportProblem>>,
    /// Where to export without asking: for tests and offscreen pictures, which can't answer a dialog.
    pub(super) target: Option<PathBuf>,
}

impl ExportCheck {
    pub(super) fn load() -> Self {
        Self { on: recent::read_setting(CHECK_FILE).is_none_or(|v| v.trim() != "off"), warning: false, refused: None, target: None }
    }

    pub(super) fn should_warn(&self, watch: &Watch) -> bool {
        self.on && watch.visible().any(|f| f.kind.is_accessibility())
    }
}

/// What Typst says, in plain words in the language on show. Its other messages go as they are.
pub fn plain_words(message: &str) -> String {
    if message.contains("missing document title") {
        tx("ua-missing-title").to_string()
    } else if message.contains("missing document language") {
        tx("ua-missing-language").to_string()
    } else if message.contains("first heading must be of level 1") {
        tx("ua-first-heading").to_string()
    } else if message.contains("skipped from heading level") {
        tx("ua-skipped-heading").to_string()
    } else if message.contains("missing alt text") {
        tx("ua-missing-alt-text").to_string()
    } else {
        tr!("ua-other", message = message)
    }
}

impl App {
    /// Exports here from now on instead of asking, and whether the warning comes first stays as it is.
    pub fn set_export_target(&mut self, path: PathBuf) {
        self.export_check.target = Some(path);
    }

    pub fn export_warning_open(&self) -> bool {
        self.export_check.warning
    }

    /// The reasons on show for a refused export, in plain words.
    pub fn export_refusal(&self) -> Option<Vec<String>> {
        Some(self.export_check.refused.as_ref()?.iter().map(|p| plain_words(&p.message)).collect())
    }

    pub(super) fn export_check_windows(&mut self, ctx: &egui::Context, t: &Tokens) {
        self.export_warning_window(ctx, t);
        self.export_refused_window(ctx, t);
    }

    fn export_warning_window(&mut self, ctx: &egui::Context, t: &Tokens) {
        if !self.export_check.warning {
            return;
        }
        let points: Vec<String> = self.watch.visible().filter(|f| f.kind.is_accessibility()).map(crate::typography::Finding::message).collect();
        let (mut export, mut see, mut cancel) = (false, false, false);
        let mut stop_asking = !self.export_check.on;
        egui::Window::new(tx("before-exporting")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.set_width(480.0);
                ui.label(RichText::new(tr!("before-exporting-points", n = points.len())).font(theme::ui_bold(13.0)));
                ui.add_space(4.0);
                for point in points.iter().take(MOST_LISTED) {
                    ui.add(egui::Label::new(point).wrap());
                }
                if points.len() > MOST_LISTED {
                    ui.colored_label(t.text_dim, "…");
                }
                ui.add_space(6.0);
                ui.checkbox(&mut stop_asking, tx("dont-warn-before-exporting"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    export = ui.button(tx("export-anyway")).clicked();
                    see = ui.button(tx("see-them-in-the-pane")).clicked();
                    cancel = ui.button(tx("cancel")).clicked();
                });
            },
        );
        if stop_asking == self.export_check.on {
            self.export_check.on = !stop_asking;
            recent::write_setting(CHECK_FILE, if stop_asking { "off" } else { "on" });
        }
        if export || see || cancel {
            self.export_check.warning = false;
        }
        if see {
            self.side = Some(Side::Problems);
        }
        if export {
            self.choose_where_to_export(false);
        }
    }

    fn export_refused_window(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(problems) = self.export_check.refused.clone() else { return };
        let (mut jump, mut close) = (None, false);
        egui::Window::new(tx("accessible-pdf-not-written")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.set_width(480.0);
                ui.label(tx("accessible-pdf-why"));
                ui.add_space(6.0);
                for problem in &problems {
                    let words = plain_words(&problem.message);
                    let sense = if problem.byte.is_some() { Sense::click() } else { Sense::hover() };
                    let color = if problem.byte.is_some() { t.accent } else { t.text };
                    let line = ui.add(egui::Label::new(RichText::new(words).color(color)).wrap().sense(sense));
                    if problem.byte.is_some() && line.on_hover_text(tx("go-to-the-passage")).clicked() {
                        jump = problem.byte;
                    }
                }
                ui.add_space(8.0);
                close = ui.button(tx("close")).clicked();
            },
        );
        if let Some(byte) = jump {
            self.jump_to(byte);
            close = true;
        }
        if close {
            self.export_check.refused = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typsts_words_become_plain_ones_and_the_rest_stay_as_they_are() {
        for (typst, id) in [
            ("PDF/UA-1 error: missing document title", "ua-missing-title"),
            ("PDF/UA-1 error: missing document language", "ua-missing-language"),
            ("PDF/UA-1 error: missing alt text", "ua-missing-alt-text"),
            ("PDF/UA-1 error: skipped from heading level 1 to 3", "ua-skipped-heading"),
            ("PDF/UA-1 error: the first heading must be of level 1", "ua-first-heading"),
        ] {
            assert_eq!(plain_words(typst), tx(id));
        }
        assert!(plain_words("PDF/UA-1 error: something new").contains("something new"));
    }
}
