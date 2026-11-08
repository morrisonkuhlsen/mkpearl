//! The splash screen: a small undecorated window before the main one, in the layout of MK Perth's.
//! On ink, Pearl's mark, name, motto and version, and a bar that fills while it shows with the
//! program's parts named in turn; beside them, the picture, whole, as tall as the window. The bar
//! is a timed presentation, not a measure of loading. It closes by itself after a moment; a click
//! or Esc closes it sooner. The picture stays alive meanwhile: its clouds drift, cut out of it when it
//! loads; the duck bobs, the fish breathes bubbles and the frog blinks.

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
const ON_INK: Color32 = Color32::WHITE;
const ON_INK_SOFT: Color32 = Color32::from_rgb(0xB8, 0xB8, 0xB8);
/// The sky of the picture, which tells its clouds from what's behind them.
const SKY: Color32 = Color32::from_rgb(0x9A, 0xE1, 0xFC);
/// The picture beside the words; square.
const PICTURE: &[u8] = include_bytes!("../assets/splash.png");
/// A cloud in the picture.
struct Cloud {
    /// The pixels that hold it, with sky all around: left, top, right, bottom, the last two past its end.
    area: [u32; 4],
    /// How fast it drifts, in widths of the picture per second. Always to the right: the last cloud
    /// runs off the picture's edge, and moving the other way would show the part it hasn't got.
    speed: f32,
}
/// The small one looks farther away, and so slower.
const CLOUDS: [Cloud; 3] = [
    Cloud { area: [45, 185, 515, 400], speed: 0.012 },
    Cloud { area: [590, 405, 890, 555], speed: 0.008 },
    Cloud { area: [920, 220, 1254, 415], speed: 0.014 },
];
/// How far a pixel may be from the sky, over its three channels, and still be sky.
const NEAR_SKY: u32 = 24;
/// The dark of the drawn lines.
const LINE: [u8; 3] = [0x10, 0x10, 0x14];
/// How far from the sky around a cloud its line's edge blends into it, in pixels.
const LINE_EDGE: usize = 2;
/// The picture's width and height, in pixels; what follows is measured on it.
const PICTURE_SIDE: f32 = 1254.0;
/// The pixels that hold the duck: left, top, right, bottom, the last two past its end.
const DUCK_AREA: [u32; 4] = [212, 755, 358, 860];
/// A point of its white body and of its orange beak, from which the rest of it is found.
const DUCK_BODY: [u32; 2] = [300, 825];
const DUCK_BEAK: [u32; 2] = [332, 790];
/// How far from its colours the duck reaches: all that's within the first, and what's dark within
/// the second, which is its drawn line.
const DUCK_EDGE: usize = 2;
const DUCK_LINE: usize = 5;
/// How bright a pixel is when it has nothing of the line: darker than the darkest of the water.
const LINE_CLEAR: f32 = 115.0;
/// How far it bobs, in pixels, and in how many seconds it goes up and down.
const DUCK_BOB: f32 = 4.0;
const DUCK_BOB_SECONDS: f32 = 2.2;
/// How much it leans, in pixels of height for each one from its middle, and how slowly.
const DUCK_LEAN: f32 = 0.014;
const DUCK_LEAN_SECONDS: f32 = 3.1;
/// The fish's mouth, where the bubbles start, and how high they go before they're gone.
const FISH_MOUTH: [f32; 2] = [675.0, 934.0];
const BUBBLE_RISE: f32 = 105.0;
/// The bubbles drawn in the picture, which those drawn here replace: x, y and radius.
const DRAWN_BUBBLES: [[f32; 3]; 2] = [[681.0, 921.0, 8.0], [692.0, 903.0, 9.0]];
const BUBBLES: usize = 3;
const BUBBLE_SECONDS: f32 = 2.4;
const BUBBLE_LINE: Color32 = Color32::from_rgb(0x1B, 0x45, 0x77);
/// The frog's eye and the lid that closes over it, in the green around the eye.
const FROG_EYE: [f32; 2] = [1128.0, 868.0];
const FROG_LID_RADIUS: f32 = 8.0;
const FROG_LID: Color32 = Color32::from_rgb(0x70, 0xB5, 0x34);
/// When the frog blinks, in seconds since the start, and how long it takes.
const BLINKS: [f32; 3] = [1.3, 3.2, 3.6];
const BLINK_SECONDS: f32 = 0.22;
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
    picture: Option<Option<Scene>>,
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
        let wordmark = self.wordmark.get_or_insert_with(|| load_wordmark(&ctx)).as_ref();
        paint(&ui.painter().with_clip_rect(rect), rect, progress, picture, wordmark);
        let done = progress >= 1.0 || response.clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if done && !self.closed {
            self.closed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

/// The picture without its clouds, and each of them to draw over it.
struct Scene {
    ground: TextureHandle,
    clouds: Vec<Layer>,
    duck: Option<Layer>,
}

struct Layer {
    texture: TextureHandle,
    /// Where it lies when it hasn't moved, as a part of the picture.
    place: Rect,
    /// How fast it drifts; the duck stays where it is.
    speed: f32,
}

fn decode_picture() -> Result<image::RgbImage, image::ImageError> {
    Ok(image::load_from_memory_with_format(PICTURE, image::ImageFormat::Png)?.to_rgb8())
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter().zip(b).map(|(x, y)| u32::from(x.abs_diff(y))).sum()
}

fn brightness([r, g, b]: [u8; 3]) -> f32 {
    0.3 * f32::from(r) + 0.59 * f32::from(g) + 0.11 * f32::from(b)
}

fn pixel(picture: &image::RgbImage, x: u32, y: u32) -> Option<[u8; 3]> {
    picture.get_pixel_checked(x, y).map(|p| p.0)
}

/// The sky's mean colour over some columns, around a row. Rows count with their neighbours, or the
/// paper's grain would draw lines across the patch left by a cloud.
fn sky_mean(picture: &image::RgbImage, columns: std::ops::Range<u32>, y: u32) -> Option<[f32; 3]> {
    const ROWS: u32 = 4;
    let mut seen = Vec::new();
    for row in y.saturating_sub(ROWS)..=y.saturating_add(ROWS) {
        seen.extend(columns.clone().filter_map(|x| pixel(picture, x, row)));
    }
    let n = seen.len() as f32;
    (n > 0.0).then(|| [0, 1, 2].map(|c: usize| seen.iter().map(|p| f32::from(p.get(c).copied().unwrap_or(0))).sum::<f32>() / n))
}

/// The sky along a row of a cloud's area, from what shows on both sides: from one colour to another.
fn sky_along(picture: &image::RgbImage, left: u32, right: u32, y: u32) -> ([f32; 3], [f32; 3]) {
    const WIDE: u32 = 8;
    let before = sky_mean(picture, left.saturating_sub(WIDE)..left, y);
    let after = sky_mean(picture, right..right.saturating_add(WIDE), y);
    match (before, after) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) | (None, Some(a)) => (a, a),
        (None, None) => {
            let sky = [f32::from(SKY.r()), f32::from(SKY.g()), f32::from(SKY.b())];
            (sky, sky)
        }
    }
}

fn mix((a, b): ([f32; 3], [f32; 3]), t: f32) -> [u8; 3] {
    [0, 1, 2].map(|c: usize| {
        let (from, to) = (a.get(c).copied().unwrap_or(0.0), b.get(c).copied().unwrap_or(0.0));
        (from + (to - from) * t).round() as u8
    })
}

/// Takes a cloud out of the picture, leaving sky, and gives it back on a clear ground. What's sky is
/// what can be reached from the area's border without leaving the sky's colour, so a gap in the drawn
/// line lets nothing through: the cloud's white isn't near that colour either.
fn cut_cloud(picture: &mut image::RgbImage, cloud: &Cloud) -> Option<(egui::ColorImage, Rect)> {
    let [left, top, right, bottom] = cloud.area;
    let (right, bottom) = (right.min(picture.width()), bottom.min(picture.height()));
    let (width, height) = (right.checked_sub(left)? as usize, bottom.checked_sub(top)? as usize);
    let across = right.saturating_sub(left).max(1) as f32;
    let rows: Vec<_> = (top..bottom).map(|y| sky_along(picture, left, right, y)).collect();
    let sky = |x: usize, y: usize| rows.get(y).map(|&row| mix(row, x as f32 / across));
    let seen = |x: usize, y: usize| pixel(picture, left.saturating_add(x as u32), top.saturating_add(y as u32));

    // Moving past the edges wraps or overshoots, and stops at the check below.
    let mut outside = vec![false; width.checked_mul(height)?];
    let mut pending: Vec<(usize, usize)> = Vec::new();
    pending.extend((0..width).flat_map(|x| [(x, 0), (x, height.saturating_sub(1))]));
    pending.extend((0..height).flat_map(|y| [(0, y), (width.saturating_sub(1), y)]));
    while let Some((x, y)) = pending.pop() {
        if x >= width || y >= height {
            continue;
        }
        let near = seen(x, y).zip(sky(x, y)).is_some_and(|(p, s)| distance(p, s) < NEAR_SKY);
        let Some(mark) = outside.get_mut(y * width + x) else { continue };
        if *mark || !near {
            continue;
        }
        *mark = true;
        pending.extend([(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))]);
    }

    let is_outside = |x: usize, y: usize| outside.get(y * width + x).copied().unwrap_or(false);
    let any_within = |x: usize, y: usize, wanted: bool| {
        (y.saturating_sub(LINE_EDGE)..=(y + LINE_EDGE).min(height - 1))
            .any(|y| (x.saturating_sub(LINE_EDGE)..=(x + LINE_EDGE).min(width - 1)).any(|x| is_outside(x, y) == wanted))
    };
    let mut image = egui::ColorImage::filled([width, height], Color32::TRANSPARENT);
    let mut patch = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let (Some(p), Some(s)) = (seen(x, y), sky(x, y)) else { continue };
            let sky_here = is_outside(x, y);
            // Sky with nothing of the cloud close by stays as it is.
            if sky_here && !any_within(x, y, false) {
                continue;
            }
            patch.push((x as u32, y as u32, s));
            let [r, g, b] = p;
            let lit = brightness(s);
            let color = if (sky_here || any_within(x, y, true)) && brightness(p) < lit {
                // Where the line meets the sky a pixel is a mix of them; here it's the line, partly clear.
                let alpha = (lit - brightness(p)) / (lit - brightness(LINE)).max(1.0);
                let [r, g, b] = LINE;
                Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
            } else if sky_here {
                Color32::TRANSPARENT
            } else {
                Color32::from_rgb(r, g, b)
            };
            if let Some(slot) = image.pixels.get_mut(y * width + x) {
                *slot = color;
            }
        }
    }
    for (x, y, s) in patch {
        if let Some(p) = picture.get_pixel_mut_checked(left + x, top + y) {
            p.0 = s;
        }
    }
    let (w, h) = (picture.width().max(1) as f32, picture.height().max(1) as f32);
    Some((image, Rect::from_min_max(pos2(left as f32 / w, top as f32 / h), pos2(right as f32 / w, bottom as f32 / h))))
}

/// Paints a circle of the picture over with the mean of what's around it.
fn erase_circle(picture: &mut image::RgbImage, [x, y, radius]: [f32; 3]) {
    const AROUND: f32 = 3.0;
    let reach = (radius + AROUND).ceil() as i64;
    let (cx, cy) = (x as i64, y as i64);
    let offsets = || (-reach..=reach).flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)));
    let far = |dx: i64, dy: i64| ((dx * dx + dy * dy) as f32).sqrt();
    let spot = |dx: i64, dy: i64| Some((u32::try_from(cx + dx).ok()?, u32::try_from(cy + dy).ok()?));
    let ring: Vec<[u8; 3]> = offsets()
        .filter(|&(dx, dy)| far(dx, dy) > radius && far(dx, dy) <= radius + AROUND)
        .filter_map(|(dx, dy)| spot(dx, dy).and_then(|(px, py)| pixel(picture, px, py)))
        .collect();
    let n = ring.len() as f32;
    if n == 0.0 {
        return;
    }
    let mean = [0, 1, 2].map(|c: usize| (ring.iter().map(|p| f32::from(p.get(c).copied().unwrap_or(0))).sum::<f32>() / n).round() as u8);
    for (dx, dy) in offsets().filter(|&(dx, dy)| far(dx, dy) <= radius) {
        if let Some((px, py)) = spot(dx, dy)
            && let Some(p) = picture.get_pixel_mut_checked(px, py)
        {
            p.0 = mean;
        }
    }
}

/// How far from nothing (0) to everything (1) a value has come between two others, gently.
fn ease(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Takes the duck out of the picture, leaving what's beside it on each row, and gives it back on a
/// clear ground. Nothing around it has one colour to tell it by, as the sky does for a cloud, so it's
/// found from inside, by its own colours.
fn cut_duck(picture: &mut image::RgbImage) -> Option<(egui::ColorImage, Rect)> {
    let [left, top, right, bottom] = DUCK_AREA;
    let (right, bottom) = (right.min(picture.width()), bottom.min(picture.height()));
    let (width, height) = (right.checked_sub(left)? as usize, bottom.checked_sub(top)? as usize);
    let seen = |x: usize, y: usize| pixel(picture, left.saturating_add(x as u32), top.saturating_add(y as u32));
    type Belongs = fn([u8; 3]) -> bool;
    let white: Belongs = |[r, g, b]| r > 200 && g > 200 && b > 190;
    let orange: Belongs = |[r, _, b]| r > 200 && b < 120;

    // Its colours, spreading from a point of each.
    let mut own = vec![false; width.checked_mul(height)?];
    for ([x, y], belongs) in [(DUCK_BODY, white), (DUCK_BEAK, orange)] {
        let mut pending = vec![(x.checked_sub(left)? as usize, y.checked_sub(top)? as usize)];
        while let Some((x, y)) = pending.pop() {
            if x >= width || y >= height {
                continue;
            }
            let fits = seen(x, y).is_some_and(belongs);
            let Some(mark) = own.get_mut(y * width + x) else { continue };
            if *mark || !fits {
                continue;
            }
            *mark = true;
            pending.extend([(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))]);
        }
    }
    let within = |x: usize, y: usize, reach: usize| {
        (y.saturating_sub(reach)..=(y + reach).min(height - 1)).any(|row| {
            (x.saturating_sub(reach)..=(x + reach).min(width - 1))
                .any(|column| column.abs_diff(x).pow(2) + row.abs_diff(y).pow(2) <= reach.pow(2) && own.get(row * width + column) == Some(&true))
        })
    };

    let mut image = egui::ColorImage::filled([width, height], Color32::TRANSPARENT);
    for y in 0..height {
        for x in 0..width {
            let Some(p) = seen(x, y) else { continue };
            let [r, g, b] = p;
            let color = if within(x, y, DUCK_EDGE) {
                Color32::from_rgb(r, g, b)
            } else if within(x, y, DUCK_LINE) {
                // The line's edge goes clear as it turns into what's behind it.
                let alpha = ((LINE_CLEAR - brightness(p)) / 40.0).clamp(0.0, 1.0);
                let [r, g, b] = LINE;
                Color32::from_rgba_unmultiplied(r, g, b, (alpha * 255.0).round() as u8)
            } else {
                continue;
            };
            if let Some(slot) = image.pixels.get_mut(y * width + x) {
                *slot = color;
            }
        }
    }

    // Behind it, row by row, from one side of each gap to the other. It moves little: what shows of
    // this is a sliver.
    // A little past the line, whose faint edge would stay behind as a ghost of the duck.
    let is_taken = |x: usize, y: usize| within(x, y, DUCK_LINE + 2);
    let mut patch = Vec::new();
    for y in 0..height {
        let mut x = 0;
        while x < width {
            if !is_taken(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && is_taken(x, y) {
                x += 1;
            }
            // A pixel of margin on each side, as a mix with the line would carry it along.
            let before = start.checked_sub(2).and_then(|at| seen(at, y));
            let after = seen(x + 1, y).filter(|_| x + 1 < width);
            let (Some(a), Some(b)) = (before.or(after), after.or(before)) else { continue };
            let ends = (a.map(f32::from), b.map(f32::from));
            for at in start..x {
                patch.push((at, y, mix(ends, (at - start) as f32 / (x - start).max(1) as f32)));
            }
        }
    }
    for (x, y, color) in patch {
        if let Some(p) = picture.get_pixel_mut_checked(left + x as u32, top + y as u32) {
            p.0 = color;
        }
    }
    let (w, h) = (picture.width().max(1) as f32, picture.height().max(1) as f32);
    Some((image, Rect::from_min_max(pos2(left as f32 / w, top as f32 / h), pos2(right as f32 / w, bottom as f32 / h))))
}

/// How far up a column of the duck has gone, in pixels: all of it bobs, and it leans a little.
fn duck_lift(x: f32, seconds: f32) -> f32 {
    let turn = |period: f32| (seconds / period * std::f32::consts::TAU).sin();
    let [left, _, right, _] = DUCK_AREA;
    let middle = (left + right) as f32 / 2.0;
    DUCK_BOB * turn(DUCK_BOB_SECONDS) + DUCK_LEAN * (x - middle) * turn(DUCK_LEAN_SECONDS)
}

/// The duck where it is now: four corners are enough, as it leans in a straight line.
fn paint_duck(painter: &Painter, duck: &Layer, art: Rect, seconds: f32) {
    let place = duck.place;
    let scale = art.width() / PICTURE_SIDE;
    let corner = |u: f32, v: f32| {
        let at = place.min.to_vec2() + vec2(u, v) * place.size();
        Vertex { pos: art.min + at * art.size() - vec2(0.0, duck_lift(at.x * PICTURE_SIDE, seconds) * scale), uv: pos2(u, v), color: Color32::WHITE }
    };
    let mut mesh = Mesh::with_texture(duck.texture.id());
    mesh.vertices.extend([corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)]);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// A bubble at this moment: where it is, its radius and how much of it shows.
fn bubble(index: usize, seconds: f32) -> ([f32; 2], f32, f32) {
    let start = index as f32 / BUBBLES as f32;
    let age = (seconds / BUBBLE_SECONDS + start).rem_euclid(1.0);
    let [x, y] = FISH_MOUTH;
    let sway = (age * 9.0 + index as f32 * 2.0).sin() * 4.0 + age * 18.0;
    let shows = ease(0.0, 0.08, age) * (1.0 - ease(0.7, 1.0, age));
    ([x + sway, y - age * BUBBLE_RISE], 2.5 + age * 5.5, shows)
}

/// How closed the frog's eye is, from 0 to 1.
fn eye_closed(seconds: f32) -> f32 {
    BLINKS.iter().map(|at| (1.0 - (seconds - at).abs() / (BLINK_SECONDS / 2.0)).max(0.0)).fold(0.0, f32::max)
}

fn paint_bubbles(painter: &Painter, art: Rect, seconds: f32) {
    let scale = art.width() / PICTURE_SIDE;
    for index in 0..BUBBLES {
        let ([x, y], radius, shows) = bubble(index, seconds);
        let center = art.min + vec2(x, y) * scale;
        let line = egui::Stroke::new(2.4 * scale, BUBBLE_LINE.gamma_multiply(shows));
        painter.circle(center, radius * scale, Color32::WHITE.gamma_multiply(0.18 * shows), line);
    }
}

fn paint_blink(painter: &Painter, art: Rect, seconds: f32) {
    let closed = eye_closed(seconds);
    if closed <= 0.0 {
        return;
    }
    let scale = art.width() / PICTURE_SIDE;
    let [x, y] = FROG_EYE;
    let center = art.min + vec2(x, y) * scale;
    let radius = FROG_LID_RADIUS * scale;
    // The lid comes down over the eye, and when it's shut its edge is a line, like the drawn ones.
    let reach = Rect::from_min_max(center - vec2(radius, radius) * 1.2, pos2(center.x + radius * 1.2, center.y - radius + 2.0 * radius * closed));
    painter.with_clip_rect(reach.intersect(art)).circle_filled(center, radius, FROG_LID);
    let points: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = (20.0 + 140.0 * i as f32 / 12.0).to_radians();
            center + vec2(a.cos(), a.sin() * 0.55) * radius * 0.85
        })
        .collect();
    painter.line(points, egui::Stroke::new(2.4 * scale, INK.gamma_multiply(ease(0.6, 1.0, closed))));
}

fn load_picture(ctx: &egui::Context) -> Option<Scene> {
    let mut picture = decode_picture().ok()?;
    for drawn in DRAWN_BUBBLES {
        erase_circle(&mut picture, drawn);
    }
    // Shown at about a third of its size: mipmaps keep the drawn lines from shimmering.
    let options = TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR };
    let mut clouds = Vec::new();
    for (i, cloud) in CLOUDS.iter().enumerate() {
        if let Some((image, place)) = cut_cloud(&mut picture, cloud) {
            clouds.push(Layer { texture: ctx.load_texture(format!("splash-cloud-{i}"), image, options), place, speed: cloud.speed });
        }
    }
    let duck = cut_duck(&mut picture).map(|(image, place)| Layer { texture: ctx.load_texture("splash-duck", image, options), place, speed: 0.0 });
    let size = [picture.width() as usize, picture.height() as usize];
    let ground = ctx.load_texture("splash-picture", egui::ColorImage::from_rgb(size, picture.as_raw()), options);
    Some(Scene { ground, clouds, duck })
}

fn decode_wordmark() -> Result<egui::ColorImage, image::ImageError> {
    let rgba = image::load_from_memory_with_format(WORDMARK, image::ImageFormat::Png)?.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

fn load_wordmark(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = decode_wordmark().ok()?;
    Some(ctx.load_texture("splash-wordmark", image, TextureOptions::LINEAR))
}

fn paint(painter: &Painter, rect: Rect, progress: f32, picture: Option<&Scene>, wordmark: Option<&TextureHandle>) {
    let k = rect.height() / LAYOUT_HEIGHT;
    // The square picture at the right, whole; the words take what is left.
    let split = rect.right() - rect.height();
    let words = Rect::from_min_max(rect.min, pos2(split, rect.bottom()));
    let art = Rect::from_min_max(pos2(split, rect.top()), rect.max);
    painter.rect_filled(rect, 0.0, INK);
    if let Some(scene) = picture {
        let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(scene.ground.id(), art, whole, Color32::WHITE);
        let seconds = progress * HOLD as f32;
        let sky = painter.with_clip_rect(art);
        if let Some(duck) = &scene.duck {
            paint_duck(&sky, duck, art, seconds);
        }
        paint_bubbles(&sky, art, seconds);
        paint_blink(&sky, art, seconds);
        for cloud in &scene.clouds {
            let min = art.min + cloud.place.min.to_vec2() * art.size() + vec2(cloud.speed * seconds * art.width(), 0.0);
            sky.image(cloud.texture.id(), Rect::from_min_size(min, cloud.place.size() * art.size()), whole, Color32::WHITE);
        }
    }

    let left = words.left() + 36.0 * k;
    let width = words.width() - 72.0 * k;
    crate::mark::paint(painter, pos2(left + 23.0 * k, words.top() + 75.0 * k), 50.0 * k, false);
    painter.text(pos2(left + 58.0 * k, words.top() + 75.0 * k), Align2::LEFT_CENTER, NAME, FontId::proportional(34.0 * k), ON_INK);
    painter.text(pos2(left, words.top() + 128.0 * k), Align2::LEFT_CENTER, MOTTO, FontId::proportional(15.0 * k), ON_INK_SOFT);
    let tagline = painter.layout(tx(TAGLINE).to_string(), FontId::proportional(12.5 * k), ON_INK_SOFT, width);
    let below = words.top() + 152.0 * k + tagline.size().y;
    painter.galley(pos2(left, words.top() + 152.0 * k), tagline, ON_INK_SOFT);
    painter.text(
        pos2(left, below + 10.0 * k),
        Align2::LEFT_TOP,
        tr!("splash-version", version = env!("CARGO_PKG_VERSION")),
        FontId::proportional(12.5 * k),
        ON_INK_SOFT,
    );

    // The parts loading, over the progress bar.
    let bar = Rect::from_min_size(pos2(left, words.bottom() - 96.0 * k), vec2(width, 3.0 * k));
    let shown = ((progress * COMPONENTS.len() as f32) as usize).min(COMPONENTS.len() - 1);
    let status = COMPONENTS.get(shown).map_or_else(String::new, |c| tr!("splash-loading", what = tx(c)));
    painter.text(bar.left_top() - vec2(0.0, 10.0 * k), Align2::LEFT_BOTTOM, status, FontId::proportional(12.0 * k), ON_INK_SOFT);
    painter.rect_filled(bar, 0.0, Color32::from_white_alpha(28));
    painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * progress, bar.height())), 0.0, BRAND);

    // The house at the foot: "powered by" and its wordmark after it.
    if let Some(texture) = wordmark {
        let foot = words.bottom() - 28.0 * k;
        let by = painter.layout_no_wrap(tx(POWERED_BY).to_string(), FontId::proportional(11.0 * k), ON_INK_SOFT);
        let logo_width = 104.0 * k;
        let size = vec2(logo_width, logo_width * texture.size_vec2().y / texture.size_vec2().x.max(1.0));
        let by_width = by.size().x;
        painter.galley(pos2(left, foot - size.y / 2.0 - by.size().y / 2.0), by, ON_INK_SOFT);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(texture.id(), Rect::from_min_size(pos2(left + by_width + 8.0 * k, foot - size.y), size), uv, ON_INK);
    }
}

/// The window icon: Pearl's mark on a clear ground.
pub fn icon() -> egui::IconData {
    icon_of_size(128)
}

/// The icon drawn at a size, for what needs several, as a Windows `.ico` does.
pub fn icon_of_size(side: u32) -> egui::IconData {
    egui::IconData { rgba: crate::mark::pixels(side), width: side, height: side }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_decodes_square_and_fits_any_gpu() {
        let image = decode_picture().unwrap();
        assert_eq!(image.width(), image.height());
        assert!(image.width() <= 2048);
    }

    #[test]
    fn the_clouds_come_out_whole_and_leave_only_sky() {
        let original = decode_picture().unwrap();
        let mut ground = original.clone();
        let layers: Vec<_> = CLOUDS.iter().map(|c| cut_cloud(&mut ground, c).unwrap()).collect();
        assert_eq!(layers.len(), 3);
        // Nothing of a cloud stays behind, its white or its line.
        for cloud in &CLOUDS {
            let [left, top, right, bottom] = cloud.area;
            for y in top..bottom {
                for x in left..right.min(ground.width()) {
                    let [r, g, b] = ground.get_pixel(x, y).0;
                    assert!(b > 200 && r < 190 && g > 180, "({x}, {y}) isn't sky: {r} {g} {b}");
                }
            }
        }
        // Each has its white and its line, and clear sky around them.
        for (image, _) in &layers {
            let [width, height] = image.size;
            assert_eq!(image[(0, 0)], Color32::TRANSPARENT);
            assert_eq!(image[(0, height - 1)], Color32::TRANSPARENT);
            let pixels = || image.pixels.iter().map(|p| p.to_array());
            let white = pixels().filter(|[r, g, b, a]| *a == 255 && *r > 230 && *g > 230 && *b > 220).count();
            let line = pixels().filter(|[r, _, _, a]| *a > 200 && *r < 60).count();
            assert!(white > width * height / 4, "white {white} of {width}x{height}");
            assert!(line > width, "line {line} of {width}x{height}");
        }
        // Still, the clouds over the ground are the picture as it was drawn.
        let mut off = 0_u64;
        let mut count = 0_u64;
        for ((image, place), cloud) in layers.iter().zip(&CLOUDS) {
            assert!(place.min.x >= 0.0 && place.max.x <= 1.0 && place.min.y >= 0.0 && place.max.y <= 1.0);
            let [left, top, ..] = cloud.area;
            let [width, height] = image.size;
            for y in 0..height {
                for x in 0..width {
                    let (px, py) = (left + x as u32, top + y as u32);
                    let [r, g, b, a] = image[(x, y)].to_array();
                    let (under, was) = (ground.get_pixel(px, py).0, original.get_pixel(px, py).0);
                    let over = |top: u8, under: u8| u32::from(top) + u32::from(under) * (255 - u32::from(a)) / 255;
                    let back = [over(r, under[0]), over(g, under[1]), over(b, under[2])];
                    off += back.iter().zip(was).map(|(p, q)| u64::from(p.abs_diff(u32::from(q)))).sum::<u64>();
                    count += 3;
                }
            }
        }
        assert!(off / count < 3, "the mean difference is {}", off as f64 / count as f64);
    }

    #[test]
    fn the_measures_are_of_this_picture() {
        assert_eq!(decode_picture().unwrap().width() as f32, PICTURE_SIDE);
    }

    #[test]
    fn the_drawn_bubbles_are_gone_and_the_water_stays() {
        let mut picture = decode_picture().unwrap();
        let dark = |picture: &image::RgbImage, [x, y, radius]: [f32; 3]| {
            let reach = radius as i32;
            (-reach..=reach)
                .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| ((dx * dx + dy * dy) as f32).sqrt() <= radius)
                .filter(|&(dx, dy)| picture.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32).0[0] < 50)
                .count()
        };
        assert!(DRAWN_BUBBLES.iter().all(|&b| dark(&picture, b) > 10), "the bubbles are where they're said to be");
        for drawn in DRAWN_BUBBLES {
            erase_circle(&mut picture, drawn);
        }
        assert!(DRAWN_BUBBLES.iter().all(|&b| dark(&picture, b) == 0));
        // Out of the picture, nothing happens and nothing breaks.
        erase_circle(&mut picture, [-500.0, 5000.0, 4.0]);
    }

    #[test]
    fn the_duck_comes_out_whole_and_the_far_shore_stays() {
        let original = decode_picture().unwrap();
        let mut ground = original.clone();
        let (image, _) = cut_duck(&mut ground).unwrap();
        let [left, top, right, bottom] = DUCK_AREA;
        // Its white, its beak and its line came with it, and nothing is left of them.
        let pixels = || image.pixels.iter().map(|p| p.to_array());
        assert!(pixels().filter(|[r, g, b, a]| *a == 255 && *r > 230 && *g > 230 && *b > 220).count() > 3000);
        assert!(pixels().filter(|[r, _, b, a]| *a == 255 && *r > 200 && *b < 120).count() > 50);
        assert!(pixels().filter(|[r, _, _, a]| *a > 200 && *r < 60).count() > 500);
        for y in top..bottom {
            for x in left..right {
                let [r, g, b] = ground.get_pixel(x, y).0;
                assert!(!(r > 225 && g > 225 && b > 215), "white left at ({x}, {y})");
                assert!(!(r > 200 && b < 100 && g < 170), "the beak left at ({x}, {y})");
            }
        }
        // It's clear at the corners, where the water and the shore are, and they aren't touched there.
        assert_eq!(image[(0, 0)], Color32::TRANSPARENT);
        assert_eq!(image[(image.size[0] - 1, 0)], Color32::TRANSPARENT);
        assert_eq!(ground.get_pixel(left, top), original.get_pixel(left, top));
        // The shore's line runs on behind its head, on a row of the patch as dark as beside it.
        let beside = ground.get_pixel(left + 2, 771).0;
        let behind = ground.get_pixel(310, 771).0;
        assert!(distance(beside, behind) < 120, "{beside:?} {behind:?}");
    }

    #[test]
    fn the_duck_bobs_and_leans_from_where_it_was_drawn() {
        assert_eq!(duck_lift(100.0, 0.0), 0.0);
        assert_eq!(duck_lift(400.0, 0.0), 0.0);
        let up = DUCK_BOB_SECONDS / 4.0;
        assert!(duck_lift(285.0, up) > DUCK_BOB * 0.9);
        let [left, _, right, _] = DUCK_AREA;
        for tenth in 0..=50 {
            let seconds = tenth as f32 / 10.0;
            for x in [left as f32, right as f32] {
                assert!(duck_lift(x, seconds).abs() <= DUCK_BOB + 2.0);
            }
        }
    }

    #[test]
    fn the_bubbles_rise_from_the_mouth_and_fade_under_the_water() {
        let surface = 785.0;
        for hundredth in 0..=1000 {
            let seconds = hundredth as f32 / 100.0;
            for index in 0..BUBBLES {
                let ([_, y], radius, shows) = bubble(index, seconds);
                assert!(y - radius > surface && y <= FISH_MOUTH[1], "{y}");
                assert!((0.0..=1.0).contains(&shows));
            }
        }
        let early = bubble(0, BUBBLE_SECONDS * 0.2);
        let later = bubble(0, BUBBLE_SECONDS * 0.6);
        assert!(later.0[1] < early.0[1] && later.1 > early.1);
        assert_eq!(bubble(0, 0.0).2, 0.0, "it starts unseen");
    }

    #[test]
    fn the_frog_blinks_now_and_then() {
        assert_eq!(eye_closed(0.0), 0.0);
        assert_eq!(eye_closed(BLINKS[0]), 1.0);
        assert_eq!(eye_closed(BLINKS[0] + BLINK_SECONDS), 0.0);
        assert!(BLINKS.iter().all(|at| *at < HOLD as f32));
    }

    #[test]
    fn a_cloud_area_outside_the_picture_gives_nothing() {
        let mut picture = image::RgbImage::new(10, 10);
        assert!(cut_cloud(&mut picture, &Cloud { area: [20, 0, 30, 5], speed: 0.0 }).is_none());
        assert!(cut_cloud(&mut picture, &Cloud { area: [0, 20, 5, 30], speed: 0.0 }).is_none());
        let empty = cut_cloud(&mut picture, &Cloud { area: [10, 10, 10, 10], speed: 0.0 });
        assert!(empty.is_none_or(|(image, _)| image.pixels.is_empty()));
    }

    #[test]
    fn the_wordmark_decodes_wide() {
        let image = decode_wordmark().unwrap();
        assert!(image.size[0] > image.size[1] * 2);
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
