//! Design tokens and fonts: the palette, metrics and style of MK Matrix's interface.
//! Adapted from MK Matrix (MIT OR Apache-2.0); see NOTICE.

use crate::i18n::tx;
use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};

pub const UI: &str = "ui";
pub const UI_BOLD: &str = "ui-bold";
// The icon set's lettered icons draw in the interface fonts here; Pearl has no cell fonts.
pub const CELL: &str = UI;
pub const CELL_BOLD: &str = UI_BOLD;
pub const CELL_ITALIC: &str = UI;

fn pick<T>(dark: bool, light: T, dark_value: T) -> T {
    if dark { dark_value } else { light }
}

pub fn ui_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(UI.into()))
}

pub fn ui_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(UI_BOLD.into()))
}

/// The platform's interface font, as MK Matrix picks it: the first candidate that loads and parses.
fn system_font(files: &[&str]) -> Option<Vec<u8>> {
    let windows = std::env::var("WINDIR").map(|w| format!("{w}\\Fonts\\")).unwrap_or_default();
    const DIRS: [&str; 4] = ["/usr/share/fonts/", "/usr/share/fonts/truetype/", "/usr/local/share/fonts/", "/System/Library/Fonts/"];
    let dirs = DIRS.iter().map(|d| (*d).to_string()).chain(std::iter::once(windows));
    dirs.flat_map(|d| files.iter().map(move |f| format!("{d}{f}"))).find_map(|path| {
        let bytes = std::fs::read(path).ok()?;
        typst::text::Font::new(typst::foundations::Bytes::new(bytes.clone()), 0)?;
        Some(bytes)
    })
}

pub fn font_definitions() -> FontDefinitions {
    const REGULAR: [&str; 6] = [
        "segoeui.ttf",
        "HelveticaNeue.ttc",
        "cantarell/Cantarell-Regular.otf",
        "noto/NotoSans-Regular.ttf",
        "dejavu/DejaVuSans.ttf",
        "liberation/LiberationSans-Regular.ttf",
    ];
    const BOLD: [&str; 4] = ["segoeuib.ttf", "noto/NotoSans-Bold.ttf", "dejavu/DejaVuSans-Bold.ttf", "liberation/LiberationSans-Bold.ttf"];
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "InterSemiBold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "Mono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    let mut regular: Vec<String> = vec!["Inter".into()];
    let mut bold: Vec<String> = vec!["InterSemiBold".into(), "Inter".into()];
    if let Some(bytes) = system_font(&REGULAR) {
        fonts.font_data.insert("system".into(), Arc::new(FontData::from_owned(bytes)));
        regular.insert(0, "system".into());
        bold.push("system".into());
    }
    if let Some(bytes) = system_font(&BOLD) {
        fonts.font_data.insert("system-bold".into(), Arc::new(FontData::from_owned(bytes)));
        bold.insert(0, "system-bold".into());
    }
    if let Some(m) = fonts.families.get_mut(&FontFamily::Monospace) {
        m.insert(0, "Mono".into());
    }
    let last_resort: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    regular.extend(last_resort.iter().cloned());
    bold.extend(last_resort);
    fonts.families.insert(FontFamily::Proportional, regular.clone());
    fonts.families.insert(FontFamily::Name(UI.into()), regular);
    fonts.families.insert(FontFamily::Name(UI_BOLD.into()), bold);
    // Typst's fonts draw the math symbols the interface fonts lack. Only these two: egui searches
    // each family's whole list, so every font added slows text layout.
    for bytes in typst_assets::fonts() {
        let data = typst::foundations::Bytes::new(bytes);
        let family = typst::text::Font::new(data, 0).map(|f| f.info().family.clone());
        if let Some(name) = family.filter(|f| matches!(f.as_str(), "New Computer Modern Math" | "DejaVu Sans Mono")) {
            fonts.font_data.insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
            for family in fonts.families.values_mut() {
                family.push(name.to_string());
            }
        }
    }
    fonts
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub window: Color32,
    pub ribbon: Color32,
    pub ribbon_border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_dark: Color32,
    pub accent_soft: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub separator: Color32,
    pub grid_bg: Color32,
    /// Behind the pages in Page Layout view.
    pub desk: Color32,
    pub gridline: Color32,
    pub header_bg: Color32,
    pub header_text: Color32,
    pub header_line: Color32,
    pub header_sel_bg: Color32,
    pub header_sel_text: Color32,
    pub header_all_bg: Color32,
    pub sel_fill: Color32,
    pub sel_border: Color32,
    pub input_bg: Color32,
    pub input_border: Color32,
    /// The formula bar's Name Box and formula field under the pointer, and while typing in them:
    /// a shade darker than the bar, and darker again when active.
    pub field_hover: Color32,
    pub field_active: Color32,
    pub tab_bar: Color32,
    pub tab_active: Color32,
    pub status_bar: Color32,
    pub cell_text: Color32,
    pub menu_bg: Color32,
    pub shadow: Color32,
    pub danger: Color32,
    /// Background error checking: the corner triangle, and the warning button's diamond and mark.
    pub error_check_green: Color32,
    pub warning_fill: Color32,
    pub warning_ink: Color32,
    /// Info-bar (notice strip) fills, by severity.
    pub notice_warn_bg: Color32,
    pub notice_warn_border: Color32,
    pub notice_info_bg: Color32,
    pub notice_info_border: Color32,
    /// Text and glyphs drawn on an `accent` fill (primary buttons, switched-on toggles).
    pub on_accent: Color32,
    /// The category palette, indexed by [`Cat`]: the coloured parts of icons, gallery highlights.
    pub cat: [Color32; 8],
    /// The page colour inside icons (documents, cells, clipboards).
    pub paper: Color32,
    /// Reference colours for formulas edited in the window-coloured formula bar.
    pub ref_colors: [Color32; 8],
    /// Reference colours on the sheet (outlines and the in-cell editor). The sheet stays white.
    pub sheet_ref_colors: [Color32; 8],
    /// Code colours in the code editor (and the window-coloured fields it sits in).
    pub syntax: Syntax,
    /// The message box warning disc and its glyph.
    pub warning: Color32,
    pub on_warning: Color32,
    /// Controls drawn on the sheet (filter and validation buttons, object handles). They follow
    /// the sheet, so they are light in both themes.
    pub sheet_control_bg: Color32,
    pub sheet_control_border: Color32,
    pub sheet_control_text: Color32,
    /// A placeholder fill on the sheet (a picture that has not loaded).
    pub sheet_track: Color32,
}

/// The eight category colours, in the order of [`Tokens::cat`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cat {
    Blue,
    Green,
    Orange,
    Red,
    Purple,
    Teal,
    Amber,
    Slate,
}

impl Cat {
    pub const ALL: [Cat; 8] = [Cat::Blue, Cat::Green, Cat::Orange, Cat::Red, Cat::Purple, Cat::Teal, Cat::Amber, Cat::Slate];
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Reference colours on the white sheet, the same in both themes (the sheet does not follow the
/// theme). The light theme's formula bar is white too, so it uses them as well.
const SHEET_REF_COLORS: [Color32; 8] =
    [rgb(0x2E6FD8), rgb(0xD03C3C), rgb(0x7A4FC9), rgb(0x23803D), rgb(0xB5531A), rgb(0x16798A), rgb(0xB83B8E), rgb(0x77681A)];
/// Code colours for one background; plain text uses the surface's own text colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Syntax {
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub builtin: Color32,
}

const LIGHT_SYNTAX: Syntax =
    Syntax { keyword: rgb(0x0B57B3), string: rgb(0xA3361B), number: rgb(0x157A5A), comment: rgb(0x6B7280), builtin: rgb(0x7B3FB5) };
const DARK_SYNTAX: Syntax =
    Syntax { keyword: rgb(0x6FAEFF), string: rgb(0xF09A7A), number: rgb(0x7FD1A8), comment: rgb(0x9AA3AE), builtin: rgb(0xC69BF2) };
const SHEET_CONTROL_BG: Color32 = rgb(0xF3F3F3);
const SHEET_CONTROL_BORDER: Color32 = rgb(0x8A8A8A);
const SHEET_CONTROL_TEXT: Color32 = rgb(0x1F1F1F);
const SHEET_TRACK: Color32 = rgb(0xD6D6D6);

impl Tokens {
    pub fn light() -> Tokens {
        Tokens::palette(false)
    }

    pub fn dark() -> Tokens {
        Tokens::palette(true)
    }

    /// One palette for both modes. The dark one is Morrison Kühlsen's ink (#14161C, the splash
    /// screen's and File view's), in cool layers of the same blue-black, with MK Matrix's teal
    /// the only colour.
    fn palette(dark: bool) -> Tokens {
        Tokens {
            dark,
            window: pick(dark, Color32::from_rgb(0xF3, 0xF3, 0xF3), Color32::from_rgb(0x14, 0x16, 0x1C)),
            ribbon: pick(dark, Color32::from_rgb(0xFF, 0xFF, 0xFF), Color32::from_rgb(0x1B, 0x1E, 0x25)),
            ribbon_border: pick(dark, Color32::from_rgb(0xE3, 0xE3, 0xE3), Color32::from_rgb(0x26, 0x2A, 0x33)),
            text: pick(dark, Color32::from_rgb(0x24, 0x24, 0x24), Color32::from_rgb(0xE6, 0xE8, 0xEC)),
            text_dim: pick(dark, Color32::from_rgb(0x61, 0x61, 0x61), Color32::from_rgb(0x9A, 0xA1, 0xAD)),
            text_disabled: pick(dark, Color32::from_rgb(0xB0, 0xB0, 0xB0), Color32::from_rgb(0x5C, 0x63, 0x70)),
            accent: pick(dark, Color32::from_rgb(0xA0, 0x52, 0x2D), Color32::from_rgb(0xE0, 0x8A, 0x5F)),
            accent_dark: pick(dark, Color32::from_rgb(0x85, 0x42, 0x22), Color32::from_rgb(0xC9, 0x71, 0x4A)),
            accent_soft: pick(dark, Color32::from_rgb(0xF5, 0xE3, 0xD8), Color32::from_rgb(0x4A, 0x2A, 0x1E)),
            hover: pick(dark, Color32::from_rgb(0xEB, 0xEB, 0xEB), Color32::from_rgb(0x23, 0x27, 0x31)),
            pressed: pick(dark, Color32::from_rgb(0xDD, 0xDD, 0xDD), Color32::from_rgb(0x2B, 0x30, 0x3B)),
            separator: pick(dark, Color32::from_rgb(0xE0, 0xE0, 0xE0), Color32::from_rgb(0x2A, 0x2E, 0x37)),
            // The sheet: white in light mode; a graphite a step lighter than the ink in dark mode.
            grid_bg: pick(dark, Color32::WHITE, Color32::from_rgb(0x2B, 0x2F, 0x38)),
            desk: pick(dark, Color32::from_rgb(0xE9, 0xE9, 0xE9), Color32::from_rgb(0x0F, 0x11, 0x15)),
            gridline: pick(dark, Color32::from_rgb(0xE1, 0xE1, 0xE1), Color32::from_rgb(0x3A, 0x3F, 0x4A)),
            // The row and column headings sit on the window's own colour.
            header_bg: pick(dark, Color32::from_rgb(0xF3, 0xF3, 0xF3), Color32::from_rgb(0x14, 0x16, 0x1C)),
            header_text: pick(dark, Color32::from_rgb(0x42, 0x42, 0x42), Color32::from_rgb(0x8B, 0x93, 0xA1)),
            header_line: pick(dark, Color32::from_rgb(0xD6, 0xD6, 0xD6), Color32::from_rgb(0x26, 0x2A, 0x33)),
            header_sel_bg: pick(dark, Color32::from_rgb(0xE1, 0xE1, 0xE1), Color32::from_rgb(0x2E, 0x24, 0x20)),
            header_sel_text: pick(dark, Color32::from_rgb(0x85, 0x42, 0x22), Color32::from_rgb(0xF0, 0xA9, 0x84)),
            header_all_bg: pick(dark, Color32::from_rgb(0xEB, 0xCD, 0xBB), Color32::from_rgb(0x5A, 0x33, 0x24)),
            sel_fill: pick(dark, Color32::from_rgba_unmultiplied(0xC0, 0x6A, 0x40, 0x22), Color32::from_rgba_unmultiplied(0xE0, 0x8A, 0x5F, 0x22)),
            sel_border: pick(dark, Color32::from_rgb(0xA0, 0x52, 0x2D), Color32::from_rgb(0xE0, 0x8A, 0x5F)),
            input_bg: pick(dark, Color32::WHITE, Color32::from_rgb(0x10, 0x12, 0x17)),
            input_border: pick(dark, Color32::from_rgb(0xD1, 0xD1, 0xD1), Color32::from_rgb(0x2E, 0x33, 0x3D)),
            field_hover: pick(dark, Color32::from_rgb(0xE8, 0xE8, 0xE8), Color32::from_rgb(0x0F, 0x11, 0x15)),
            field_active: pick(dark, Color32::from_rgb(0xDE, 0xDE, 0xDE), Color32::from_rgb(0x0A, 0x0B, 0x0E)),
            tab_bar: pick(dark, Color32::from_rgb(0xF3, 0xF3, 0xF3), Color32::from_rgb(0x14, 0x16, 0x1C)),
            tab_active: pick(dark, Color32::WHITE, Color32::from_rgb(0x1F, 0x23, 0x2B)),
            status_bar: pick(dark, Color32::from_rgb(0xF3, 0xF3, 0xF3), Color32::from_rgb(0x14, 0x16, 0x1C)),
            cell_text: pick(dark, Color32::BLACK, Color32::from_rgb(0xE6, 0xE8, 0xEC)),
            menu_bg: pick(dark, Color32::WHITE, Color32::from_rgb(0x1B, 0x1E, 0x25)),
            shadow: pick(dark, Color32::from_black_alpha(40), Color32::from_black_alpha(140)),
            danger: pick(dark, Color32::from_rgb(0xC4, 0x2B, 0x1C), Color32::from_rgb(0xFF, 0x95, 0x85)),
            error_check_green: pick(dark, Color32::from_rgb(0x1E, 0x8E, 0x3E), Color32::from_rgb(0x4C, 0xC2, 0x6B)),
            warning_fill: Color32::from_rgb(0xF2, 0xB7, 0x05),
            warning_ink: Color32::BLACK,
            notice_warn_bg: pick(dark, Color32::from_rgb(0xFF, 0xF4, 0xCE), Color32::from_rgb(0x4A, 0x3F, 0x1E)),
            notice_warn_border: pick(dark, Color32::from_rgb(0xF0, 0xC3, 0x6E), Color32::from_rgb(0x7A, 0x67, 0x30)),
            notice_info_bg: pick(dark, Color32::from_rgb(0xEA, 0xF2, 0xFB), Color32::from_rgb(0x24, 0x33, 0x44)),
            notice_info_border: pick(dark, Color32::from_rgb(0xB9, 0xD2, 0xEC), Color32::from_rgb(0x3B, 0x55, 0x72)),
            on_accent: pick(dark, Color32::WHITE, rgb(0x0A2A23)),
            cat: pick(
                dark,
                [rgb(0x2F6FCF), rgb(0x1E8A4C), rgb(0xC8601A), rgb(0xC8372D), rgb(0x8350C8), rgb(0x13808A), rgb(0x9C6C00), rgb(0x5A6B7D)],
                [rgb(0x6EA8F7), rgb(0x5CC587), rgb(0xF0965A), rgb(0xF07B6E), rgb(0xB394EE), rgb(0x4CC7CF), rgb(0xF2C14E), rgb(0xA9B6C4)],
            ),
            paper: pick(dark, Color32::WHITE, rgb(0x2A2E37)),
            ref_colors: pick(
                dark,
                SHEET_REF_COLORS,
                [rgb(0x6FA3F5), rgb(0xF27C7C), rgb(0xB394EE), rgb(0x6CCB85), rgb(0xF0A061), rgb(0x5CC9DA), rgb(0xE77FC4), rgb(0xD4C25A)],
            ),
            // On the dark sheet the references take the formula bar's dark-mode colours.
            sheet_ref_colors: pick(
                dark,
                SHEET_REF_COLORS,
                [rgb(0x6FA3F5), rgb(0xF27C7C), rgb(0xB394EE), rgb(0x6CCB85), rgb(0xF0A061), rgb(0x5CC9DA), rgb(0xE77FC4), rgb(0xD4C25A)],
            ),
            syntax: pick(dark, LIGHT_SYNTAX, DARK_SYNTAX),
            warning: rgb(0xF2C32E),
            on_warning: rgb(0x1F1F1F),
            sheet_control_bg: SHEET_CONTROL_BG,
            sheet_control_border: SHEET_CONTROL_BORDER,
            sheet_control_text: SHEET_CONTROL_TEXT,
            sheet_track: SHEET_TRACK,
        }
    }

    /// The Pearl theme: nacre. Warm ivory surfaces in layers, mauve the only colour, and code in
    /// the rose, aqua and blue a pearl's sheen shows.
    pub fn pearl() -> Tokens {
        Tokens {
            window: rgb(0xF4EFE7),
            ribbon: rgb(0xFBF8F2),
            ribbon_border: rgb(0xE6DDD0),
            text: rgb(0x2A2622),
            text_dim: rgb(0x655D54),
            text_disabled: rgb(0xB3A99C),
            accent: rgb(0x8A4F7D),
            accent_dark: rgb(0x6F3D65),
            accent_soft: rgb(0xF0DFEA),
            hover: rgb(0xEDE5D9),
            pressed: rgb(0xE3D9CA),
            separator: rgb(0xE3DACC),
            grid_bg: rgb(0xFFFDF9),
            desk: rgb(0xEDE6DB),
            gridline: rgb(0xE9E1D5),
            header_bg: rgb(0xF4EFE7),
            header_text: rgb(0x5C544B),
            header_line: rgb(0xDDD3C5),
            input_bg: rgb(0xFFFDF9),
            input_border: rgb(0xD6CBBB),
            field_hover: rgb(0xF2EBDF),
            field_active: rgb(0xE9E1D5),
            tab_bar: rgb(0xF4EFE7),
            tab_active: rgb(0xFFFDF9),
            status_bar: rgb(0xF4EFE7),
            menu_bg: rgb(0xFFFDF9),
            paper: rgb(0xFFFDF9),
            on_accent: Color32::WHITE,
            syntax: Syntax { keyword: rgb(0x7A3E8E), string: rgb(0x1A706E), number: rgb(0xA2513A), comment: rgb(0x70675C), builtin: rgb(0x2F6496) },
            ..Tokens::light()
        }
    }

    pub fn get(ctx: &egui::Context) -> Tokens {
        if ctx.global_style().visuals.dark_mode {
            Tokens::dark()
        } else if ctx.data(|d| d.get_temp::<bool>(pearl_flag_id())).unwrap_or(false) {
            Tokens::pearl()
        } else {
            Tokens::light()
        }
    }

    /// One colour of the category palette.
    pub fn cat(&self, c: Cat) -> Color32 {
        self.cat.get(c as usize).copied().unwrap_or(self.text)
    }

    /// The reference colour for a formula's `index`-th reference, on the bar or on the sheet.
    pub fn ref_color(&self, index: usize, on_sheet: bool) -> Color32 {
        let set = if on_sheet { &self.sheet_ref_colors } else { &self.ref_colors };
        set.get(index % set.len()).copied().unwrap_or(self.text)
    }
}

fn pearl_flag_id() -> egui::Id {
    egui::Id::unique("pearl_theme_on")
}

/// The themes on offer. egui has two theme slots, so Pearl lives in the light one while chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
    Pearl,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 4] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::Pearl];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => tx("system"),
            ThemeChoice::Light => tx("light"),
            ThemeChoice::Dark => tx("dark"),
            ThemeChoice::Pearl => "Pearl",
        }
    }

    /// How it is written in the settings.
    pub fn code(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
            ThemeChoice::Pearl => "pearl",
        }
    }

    pub fn parse(code: &str) -> Option<ThemeChoice> {
        ThemeChoice::ALL.into_iter().find(|c| c.code() == code.trim())
    }

    /// The saved choice, or the dark theme when none was made yet.
    pub fn load() -> ThemeChoice {
        crate::recent::read_setting("theme.txt").and_then(|t| ThemeChoice::parse(&t)).unwrap_or(ThemeChoice::Dark)
    }

    pub fn save(self) {
        crate::recent::write_setting("theme.txt", self.code());
    }
}

/// Switches the interface to `choice`.
pub fn set_choice(ctx: &egui::Context, choice: ThemeChoice) {
    let pearl = choice == ThemeChoice::Pearl;
    let light = if pearl { Tokens::pearl() } else { Tokens::light() };
    ctx.set_visuals_of(egui::Theme::Light, visuals(&light));
    ctx.data_mut(|d| d.insert_temp(pearl_flag_id(), pearl));
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light | ThemeChoice::Pearl => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}

/// egui's visuals for one palette, with the 2-3 px widget and square window corners of MK Matrix.
fn visuals(t: &Tokens) -> Visuals {
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.window;
    v.window_fill = t.menu_bg;
    v.extreme_bg_color = t.input_bg;
    v.selection.bg_fill = t.accent_soft;
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.hyperlink_color = t.accent;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.input_border);
    v.widgets.noninteractive.corner_radius = egui::CornerRadius::ZERO;
    v.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
    v.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
    v.widgets.active.corner_radius = egui::CornerRadius::ZERO;
    v.widgets.open.corner_radius = egui::CornerRadius::ZERO;
    v.window_corner_radius = egui::CornerRadius::ZERO;
    v.menu_corner_radius = egui::CornerRadius::ZERO;
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: t.shadow };
    v
}

/// Installs the interface's fonts, spacing and text styles, and the theme `choice`.
pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_fonts(font_definitions());
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&Tokens::dark()));
    // Ctrl + and Ctrl - zoom the pages (see `App::shortcuts`), not the whole interface.
    ctx.options_mut(|o| {
        o.fallback_theme = egui::Theme::Light;
        o.zoom_with_keyboard = false;
    });
    set_choice(ctx, choice);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.text_styles.insert(egui::TextStyle::Body, ui_font(13.0));
        s.text_styles.insert(egui::TextStyle::Button, ui_font(13.0));
        s.text_styles.insert(egui::TextStyle::Small, ui_font(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, ui_bold(18.0));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(13.0));
    });
}

/// What lifts a sheet of paper off what's behind it: a soft shadow and a hairline round its edge.
pub fn paint_sheet_shadow(p: &egui::Painter, sheet: egui::Rect, t: &Tokens) {
    p.add(egui::epaint::Shadow { offset: [0, 3], blur: 18, spread: 0, color: t.shadow }.as_shape(sheet, 0.0));
    p.rect_stroke(sheet, 0.0, egui::Stroke::new(1.0, Color32::from_black_alpha(24)), egui::StrokeKind::Outside);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        let lin = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// WCAG AA: 4.5:1 for text, 3:1 for glyphs, on every surface the chrome draws.
    #[test]
    fn the_pearl_theme_meets_wcag_contrast() {
        let t = Tokens::pearl();
        let mut failures = Vec::new();
        let mut check = |name: &str, fg: Color32, bg: Color32, min: f32| {
            if contrast(fg, bg) < min {
                failures.push(format!("{name}: {:.2}:1, needs {min}:1", contrast(fg, bg)));
            }
        };
        for (name, bg) in
            [("window", t.window), ("ribbon", t.ribbon), ("menu", t.menu_bg), ("input", t.input_bg), ("status", t.status_bar), ("hover", t.hover)]
        {
            check(&format!("text on {name}"), t.text, bg, 4.5);
            check(&format!("dim text on {name}"), t.text_dim, bg, 4.5);
            check(&format!("accent on {name}"), t.accent, bg, 3.0);
            check(&format!("danger on {name}"), t.danger, bg, 4.5);
        }
        check("text on pressed", t.text, t.pressed, 4.5);
        check("File tab on window", t.accent, t.window, 4.5);
        check("on accent", t.on_accent, t.accent, 4.5);
        check("line numbers", t.header_text, t.input_bg, 4.5);
        let s = t.syntax;
        for (name, c) in [("keyword", s.keyword), ("string", s.string), ("number", s.number), ("comment", s.comment), ("builtin", s.builtin)] {
            check(&format!("code {name}"), c, t.input_bg, 4.5);
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn choosing_a_theme_changes_the_tokens() {
        let ctx = egui::Context::default();
        let get = |c: ThemeChoice| {
            set_choice(&ctx, c);
            let mut tokens = None;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| tokens = Some(Tokens::get(ui.ctx())));
            output.textures_delta.clear(); // nothing consumes the atlas in a headless test
            tokens
        };
        assert_eq!(get(ThemeChoice::Pearl), Some(Tokens::pearl()));
        assert_eq!(get(ThemeChoice::Dark), Some(Tokens::dark()));
        assert_eq!(get(ThemeChoice::Light), Some(Tokens::light()));
    }

    #[test]
    fn theme_codes_round_trip() {
        for c in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::parse(c.code()), Some(c));
        }
        assert_eq!(ThemeChoice::parse("nonsense"), None);
    }
}
