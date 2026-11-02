//! The splash screen: a small undecorated window before the main one, in the layout of MK Perth's.
//! On ink, Pearl's mark, name, motto and version, and a bar that fills while it shows with the
//! program's parts named in turn; beside them, the picture, whole, as tall as the window. The bar
//! is a timed presentation, not a measure of loading. It closes by itself after a moment; a click
//! or Esc closes it sooner.

use crate::i18n::tx;
use crate::tr;
use egui::epaint::{Mesh, Vertex};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Sense, TextureHandle, TextureOptions, pos2, vec2};

/// The window's size, in points.
pub const SIZE: [f32; 2] = [816.0, 480.0];
/// The height the layout's measures are given for; they scale with the window's.
const LAYOUT_HEIGHT: f32 = 400.0;
/// Seconds it stays.
const HOLD: f64 = 5.0;
pub(crate) const INK: Color32 = Color32::from_rgb(0x14, 0x16, 0x1C);
/// The sky of the picture, which the words share.
const SKY: Color32 = Color32::from_rgb(0x9A, 0xE1, 0xFC);
const ON_SKY: Color32 = INK;
const ON_SKY_SOFT: Color32 = Color32::from_rgb(0x1F, 0x3B, 0x4A);
/// The picture beside the words; square.
const PICTURE: &[u8] = include_bytes!("../assets/splash.png");
/// Pearl's colour: nacre.
pub const BRAND: Color32 = Color32::from_rgb(0xE9, 0xE1, 0xD3);
const NAME: &str = "Pearl";
/// "What is written remains."
const MOTTO: &str = "Scripta manent";
const TAGLINE: &str = "documents-in-typst-with-the-ribbon-you";
const POWERED_BY: &str = "developed-by";
/// The Morrison Kühlsen wordmark, white on clear: a brand asset under its own terms (`docs/brand/`).
const WORDMARK: &[u8] = include_bytes!("../docs/brand/morrison-kuhlsen.png");
/// The program's parts, named in turn as the bar fills.
const COMPONENTS: [&str; 8] = ["fonts", "typst-engine", "math-2", "tables-2", "references-2", "editor-2", "ribbon", "pdf-export"];

#[derive(Default)]
pub struct SplashScreen {
    /// When it first showed, in egui's time.
    started: Option<f64>,
    closed: bool,
    /// The picture, once decoded; the inner `None` when it couldn't be.
    picture: Option<Option<TextureHandle>>,
    wordmark: Option<Option<TextureHandle>>,
}

impl SplashScreen {
    /// Whether it asked its window to close.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Draws it over the whole window; asks the window to close when its time is up.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let started = *self.started.get_or_insert(now);
        let progress = ((now - started) / HOLD).clamp(0.0, 1.0) as f32;
        // The whole window: the root Ui comes inset by a margin.
        let rect = ctx.content_rect();
        let response = ui.allocate_rect(ui.max_rect(), Sense::click());
        let picture = self.picture.get_or_insert_with(|| load_picture(&ctx)).as_ref();
        let picture = picture.cloned();
        let wordmark = self.wordmark.get_or_insert_with(|| load_wordmark(&ctx)).as_ref();
        paint(&ui.painter().with_clip_rect(rect), rect, progress, picture.as_ref(), wordmark);
        let done = progress >= 1.0 || response.clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if done && !self.closed {
            self.closed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn decode_picture() -> Result<egui::ColorImage, image::ImageError> {
    let rgb = image::load_from_memory_with_format(PICTURE, image::ImageFormat::Png)?.to_rgb8();
    Ok(egui::ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw()))
}

fn load_picture(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = decode_picture().ok()?;
    // Shown at about a third of its size: mipmaps keep the drawn lines from shimmering.
    let options = TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR };
    Some(ctx.load_texture("splash-picture", image, options))
}

fn decode_wordmark() -> Result<egui::ColorImage, image::ImageError> {
    let rgba = image::load_from_memory_with_format(WORDMARK, image::ImageFormat::Png)?.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

fn load_wordmark(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = decode_wordmark().ok()?;
    Some(ctx.load_texture("splash-wordmark", image, TextureOptions::LINEAR))
}

/// The hand-drawn family, or the plain one for as long as it isn't loaded: new fonts take a frame to
/// arrive, and asking for one that hasn't would end the program.
fn drawn_family(ctx: &egui::Context) -> egui::FontFamily {
    let family = egui::FontFamily::Name(crate::theme::CARTOON.into());
    if ctx.fonts(|fonts| fonts.families().contains(&family)) { family } else { egui::FontFamily::Proportional }
}

fn paint(painter: &Painter, rect: Rect, progress: f32, picture: Option<&TextureHandle>, wordmark: Option<&TextureHandle>) {
    let k = rect.height() / LAYOUT_HEIGHT;
    let family = drawn_family(painter.ctx());
    let drawn = |size: f32| FontId::new(size, family.clone());
    // The square picture at the right, whole; the words take what is left.
    let split = rect.right() - rect.height();
    let words = Rect::from_min_max(rect.min, pos2(split, rect.bottom()));
    let art = Rect::from_min_max(pos2(split, rect.top()), rect.max);
    painter.rect_filled(rect, 0.0, SKY);
    if let Some(texture) = picture {
        painter.image(texture.id(), art, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }

    let left = words.left() + 36.0 * k;
    let width = words.width() - 72.0 * k;
    pearl(painter, pos2(left + 23.0 * k, words.top() + 75.0 * k), 21.0 * k);
    painter.text(pos2(left + 58.0 * k, words.top() + 75.0 * k), Align2::LEFT_CENTER, NAME, drawn(42.0 * k), ON_SKY);
    painter.text(pos2(left, words.top() + 128.0 * k), Align2::LEFT_CENTER, MOTTO, drawn(19.0 * k), ON_SKY_SOFT);
    let tagline = painter.layout(tx(TAGLINE).to_string(), drawn(15.5 * k), ON_SKY_SOFT, width);
    let below = words.top() + 152.0 * k + tagline.size().y;
    painter.galley(pos2(left, words.top() + 152.0 * k), tagline, ON_SKY_SOFT);
    painter.text(
        pos2(left, below + 10.0 * k),
        Align2::LEFT_TOP,
        tr!("splash-version", version = env!("CARGO_PKG_VERSION")),
        drawn(15.5 * k),
        ON_SKY_SOFT,
    );

    // The parts loading, over the progress bar.
    let bar = Rect::from_min_size(pos2(left, words.bottom() - 96.0 * k), vec2(width, 3.0 * k));
    let shown = ((progress * COMPONENTS.len() as f32) as usize).min(COMPONENTS.len() - 1);
    let status = COMPONENTS.get(shown).map_or_else(String::new, |c| tr!("splash-loading", what = tx(c)));
    painter.text(bar.left_top() - vec2(0.0, 10.0 * k), Align2::LEFT_BOTTOM, status, drawn(15.0 * k), ON_SKY_SOFT);
    painter.rect_filled(bar, 1.0, Color32::from_black_alpha(36));
    painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * progress, bar.height())), 1.0, INK);

    // The house at the foot: "powered by" and its wordmark after it.
    if let Some(texture) = wordmark {
        let foot = words.bottom() - 28.0 * k;
        let by = painter.layout_no_wrap(tx(POWERED_BY).to_string(), drawn(14.0 * k), ON_SKY_SOFT);
        let logo_width = 104.0 * k;
        let size = vec2(logo_width, logo_width * texture.size_vec2().y / texture.size_vec2().x.max(1.0));
        let by_width = by.size().x;
        painter.galley(pos2(left, foot - size.y / 2.0 - by.size().y / 2.0), by, ON_SKY_SOFT);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let logo = Rect::from_min_size(pos2(left + by_width + 8.0 * k, foot - size.y), size);
        // The file is all white on nothing, so it takes the ink of the words on the sky.
        painter.image(texture.id(), logo, uv, ON_SKY);
    }
}

/// The colour of a sphere's point (`x`, `y` from -1 to 1 on the disc): nacre with a rose and aqua
/// sheen, lit from the top left.
fn nacre(x: f32, y: f32) -> [f32; 3] {
    let z = (1.0 - x * x - y * y).max(0.0).sqrt();
    let light = [-0.45_f32, -0.55, 0.70];
    let diffuse = (x * light[0] + y * light[1] + z * light[2]).max(0.0);
    let half = [light[0], light[1], light[2] + 1.0];
    let norm = (half[0] * half[0] + half[1] * half[1] + half[2] * half[2]).sqrt();
    let specular = ((x * half[0] + y * half[1] + z * half[2]) / norm).max(0.0).powi(70);
    let sheen = (3.2 * (x - y) + 2.4 * z).sin() * 0.5 + 0.5;
    let tint = [0.86 + 0.12 * sheen, 0.90 - 0.04 * sheen, 0.94 - 0.10 * sheen];
    // The edge turns away and goes a little dark, with a touch of light bounced from below.
    let shade = 0.62 + 0.42 * diffuse + 0.10 * (1.0 - z) * y.max(0.0);
    [0.0, 1.0, 2.0].map(|c: f32| {
        let c = c as usize;
        let base = [0.93, 0.89, 0.83];
        (base.get(c).copied().unwrap_or(1.0) * tint.get(c).copied().unwrap_or(1.0) * shade + 0.55 * specular).clamp(0.0, 1.0)
    })
}

fn to_color([r, g, b]: [f32; 3]) -> Color32 {
    let byte = |v: f32| (v * 255.0).round() as u8;
    Color32::from_rgb(byte(r), byte(g), byte(b))
}

/// Pearl's mark: a shaded sphere, as a mesh of rings around its centre.
pub fn pearl(painter: &Painter, center: Pos2, radius: f32) {
    const RINGS: u32 = 28;
    const SEGMENTS: u32 = 72;
    // The antialiased edge; the mesh sits just inside it.
    painter.circle_filled(center, radius, to_color(nacre(0.0, 0.998)));
    let inner = (radius - 0.6).max(0.5);
    let mut mesh = Mesh::default();
    let vertex = |x: f32, y: f32| Vertex { pos: center + vec2(x, y) * inner, uv: egui::epaint::WHITE_UV, color: to_color(nacre(x, y)) };
    mesh.vertices.push(vertex(0.0, 0.0));
    for ring in 1..=RINGS {
        // Rings crowd towards the edge, where the shading changes fastest.
        let r = (ring as f32 / RINGS as f32 * std::f32::consts::FRAC_PI_2).sin();
        for s in 0..SEGMENTS {
            let a = s as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            mesh.vertices.push(vertex(r * a.cos(), r * a.sin()));
        }
    }
    let at = |ring: u32, s: u32| 1 + (ring - 1) * SEGMENTS + s % SEGMENTS;
    for s in 0..SEGMENTS {
        mesh.add_triangle(0, at(1, s), at(1, s + 1));
    }
    for ring in 1..RINGS {
        for s in 0..SEGMENTS {
            mesh.add_triangle(at(ring, s), at(ring, s + 1), at(ring + 1, s));
            mesh.add_triangle(at(ring, s + 1), at(ring + 1, s + 1), at(ring + 1, s));
        }
    }
    painter.add(mesh);
}

/// The window icon: the same sphere on a clear ground.
pub fn icon() -> egui::IconData {
    const SIDE: u32 = 128;
    let mut rgba = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    let half = SIDE as f32 / 2.0;
    for py in 0..SIDE {
        for px in 0..SIDE {
            let (x, y) = ((px as f32 + 0.5 - half) / (half - 4.0), (py as f32 + 0.5 - half) / (half - 4.0));
            let d = (x * x + y * y).sqrt();
            let alpha = ((1.0 - d) * (half - 4.0) + 0.5).clamp(0.0, 1.0);
            let scale = if d > 0.999 { 0.999 / d } else { 1.0 };
            let [r, g, b, _] = to_color(nacre(x * scale, y * scale)).to_array();
            rgba.extend_from_slice(&[r, g, b, (alpha * 255.0) as u8]);
        }
    }
    egui::IconData { rgba, width: SIDE, height: SIDE }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_decodes_square_and_fits_any_gpu() {
        let image = decode_picture().unwrap();
        assert_eq!(image.size[0], image.size[1]);
        assert!(image.size[0] <= 2048);
    }

    #[test]
    fn the_wordmark_decodes_wide() {
        let image = decode_wordmark().unwrap();
        assert!(image.size[0] > image.size[1] * 2);
    }

    #[test]
    fn nacre_stays_in_range_over_the_disc() {
        for i in -20..=20 {
            for j in -20..=20 {
                for v in nacre(i as f32 / 20.0, j as f32 / 20.0) {
                    assert!((0.0..=1.0).contains(&v));
                }
            }
        }
    }

    #[test]
    fn the_icon_has_every_pixel_and_a_clear_corner() {
        let icon = icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert_eq!(icon.rgba.get(3), Some(&0));
    }

    #[test]
    fn it_closes_on_esc_and_after_its_time() {
        let size = egui::vec2(SIZE[0], SIZE[1]);
        let mut harness =
            egui_kittest::Harness::builder().with_size(size).build_ui_state(|ui, s: &mut SplashScreen| s.ui(ui), SplashScreen::default());
        harness.run_steps(2);
        assert!(!harness.state().closed());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(1);
        assert!(harness.state().closed());
        let mut waiting = egui_kittest::Harness::builder()
            .with_size(size)
            .with_step_dt(1.0)
            .build_ui_state(|ui, s: &mut SplashScreen| s.ui(ui), SplashScreen::default());
        waiting.run_steps(7);
        assert!(waiting.state().closed(), "gone after {HOLD} s");
    }
}
