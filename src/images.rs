//! Pictures brought into a document. Typst reads only from the document's folder, so a picture from
//! anywhere else is copied there first, and one already inside is used where it is.
//!
//! Failures are the id of the message that explains them.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};

pub const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "svg", "webp"];
/// Where copies go, inside the document's folder.
const FOLDER: &str = "images";
/// As much as a session sends to its guests: a bigger picture would break the document for them.
const MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Other names tried when one is taken by a different picture.
const MAX_NAMES: u32 = 1000;

pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|known| e.eq_ignore_ascii_case(known)))
}

/// The files a pasted text stands for: what a file manager puts on the clipboard, one per line, as
/// `file://` addresses or full paths. `None` when any line is something else, which makes it text.
pub fn files_in_text(text: &str) -> Option<Vec<PathBuf>> {
    let lines = || text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    let files = lines().map(file_of_line).collect::<Option<Vec<_>>>()?;
    (!files.is_empty()).then_some(files)
}

fn file_of_line(line: &str) -> Option<PathBuf> {
    let Some(rest) = line.strip_prefix("file://") else {
        let path = Path::new(line);
        return (path.is_absolute() && path.is_file()).then(|| path.to_path_buf());
    };
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(rest)?;
    // `file:///C:/foto.png` on Windows.
    let decoded = if cfg!(windows) { decoded.strip_prefix('/').unwrap_or(&decoded).to_string() } else { decoded };
    Some(PathBuf::from(decoded))
}

fn percent_decode(text: &str) -> Option<String> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let high = char::from(bytes.next()?).to_digit(16)?;
            let low = char::from(bytes.next()?).to_digit(16)?;
            out.push(u8::try_from(high * 16 + low).ok()?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

/// The picture's path from the document's folder `dir`, copying it there if it's outside.
pub fn adopt(dir: &Path, file: &Path) -> Result<String, &'static str> {
    if !is_image(file) {
        return Err("only-images-can-be-inserted");
    }
    let meta = fs::metadata(file).map_err(|_| "the-image-could-not-be-read")?;
    if !meta.is_file() {
        return Err("the-image-could-not-be-read");
    }
    if let (Ok(root), Ok(real)) = (dir.canonicalize(), file.canonicalize())
        && let Ok(inside) = real.strip_prefix(&root)
    {
        return Ok(portable(inside));
    }
    if meta.len() > MAX_BYTES {
        return Err("the-image-is-too-large");
    }
    let name = file.file_name().and_then(|n| n.to_str()).ok_or("the-image-could-not-be-copied")?;
    let bytes = fs::read(file).map_err(|_| "the-image-could-not-be-read")?;
    store(dir, name, &bytes)
}

/// Saves a picture that came from the clipboard, named by `stamp`.
pub fn adopt_pixels(dir: &Path, image: &egui::ColorImage, stamp: &str) -> Result<String, &'static str> {
    let [width, height] = image.size;
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else { return Err("the-image-is-too-large") };
    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    let mut png = Vec::new();
    // Fast: this runs between two frames, and a screenshot is a large picture.
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
        .write_image(&pixels, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|_| "the-image-could-not-be-copied")?;
    if u64::try_from(png.len()).map_or(true, |n| n > MAX_BYTES) {
        return Err("the-image-is-too-large");
    }
    store(dir, &format!("{stamp}.png"), &png)
}

fn store(dir: &Path, name: &str, bytes: &[u8]) -> Result<String, &'static str> {
    keep(&dir.join(FOLDER), name, bytes).map(|kept| format!("{FOLDER}/{kept}")).ok_or("the-image-could-not-be-copied")
}

/// Writes into a folder under a free name, never over another file, and returns the name. The same
/// contents brought in again are found under their name instead of copied a second time.
pub(crate) fn keep(folder: &Path, name: &str, bytes: &[u8]) -> Option<String> {
    fs::create_dir_all(folder).ok()?;
    let plain = Path::new(name);
    let stem = plain.file_stem()?.to_str()?;
    let extension = plain.extension()?.to_str()?;
    for n in 1..MAX_NAMES {
        let candidate = if n == 1 { name.to_string() } else { format!("{stem}-{n}.{extension}") };
        let path = folder.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if file.write_all(bytes).is_err() {
                    drop(file);
                    // Only what was just created, and half written.
                    let _ = fs::remove_file(&path);
                    return None;
                }
                return Some(candidate);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64) && fs::read(&path).is_ok_and(|old| old == bytes) {
                    return Some(candidate);
                }
            }
            Err(_) => return None,
        }
    }
    None
}

/// A path from the document's folder as Typst writes it, whatever the system.
pub(crate) fn portable(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pearl-images-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_picture_inside_stays_where_it_is() {
        let root = folder("inside");
        fs::create_dir_all(root.join("fig")).unwrap();
        fs::write(root.join("fig/a.PNG"), b"x").unwrap();
        assert_eq!(adopt(&root, &root.join("fig/a.PNG")), Ok("fig/a.PNG".to_string()));
        assert!(!root.join("images").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_picture_outside_is_copied_and_nothing_is_overwritten() {
        let (root, away) = (folder("doc"), folder("away"));
        fs::write(away.join("foto.png"), b"one").unwrap();
        assert_eq!(adopt(&root, &away.join("foto.png")), Ok("images/foto.png".to_string()));
        // The same picture again is the same file.
        assert_eq!(adopt(&root, &away.join("foto.png")), Ok("images/foto.png".to_string()));
        // Another with that name gets its own, and the first is untouched.
        fs::write(away.join("foto.png"), b"two").unwrap();
        assert_eq!(adopt(&root, &away.join("foto.png")), Ok("images/foto-2.png".to_string()));
        assert_eq!(fs::read(root.join("images/foto.png")).unwrap(), b"one");
        assert_eq!(fs::read(root.join("images/foto-2.png")).unwrap(), b"two");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(away).unwrap();
    }

    #[test]
    fn what_cannot_be_brought_in_says_why() {
        let (root, away) = (folder("refused"), folder("refused-away"));
        fs::write(away.join("notas.txt"), b"x").unwrap();
        assert_eq!(adopt(&root, &away.join("notas.txt")), Err("only-images-can-be-inserted"));
        assert_eq!(adopt(&root, &away.join("nada.png")), Err("the-image-could-not-be-read"));
        fs::create_dir_all(away.join("pasta.png")).unwrap();
        assert_eq!(adopt(&root, &away.join("pasta.png")), Err("the-image-could-not-be-read"));
        let big = fs::File::create(away.join("grande.png")).unwrap();
        big.set_len(MAX_BYTES + 1).unwrap();
        assert_eq!(adopt(&root, &away.join("grande.png")), Err("the-image-is-too-large"));
        // A file where the pictures' folder goes.
        fs::write(root.join("images"), b"x").unwrap();
        fs::write(away.join("foto.png"), b"x").unwrap();
        assert_eq!(adopt(&root, &away.join("foto.png")), Err("the-image-could-not-be-copied"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(away).unwrap();
    }

    #[test]
    fn pixels_become_a_png_with_their_transparency() {
        let root = folder("pixels");
        let picture = egui::ColorImage::from_rgba_unmultiplied([2, 1], &[255, 0, 0, 255, 0, 0, 255, 128]);
        let path = adopt_pixels(&root, &picture, "2026-11-01-120000").unwrap();
        assert_eq!(path, "images/2026-11-01-120000.png");
        let read = image::open(root.join(&path)).unwrap().to_rgba8();
        assert_eq!(read.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(read.get_pixel(1, 0).0[3], 128);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_pasted_text_stands_for_files_only_when_every_line_is_one() {
        let base = if cfg!(windows) { "C:/tmp" } else { "/tmp" };
        let address = if cfg!(windows) { "/C:/tmp" } else { "/tmp" };
        let expected = [PathBuf::from(format!("{base}/uma foto.png")), PathBuf::from(format!("{base}/ç.png"))];
        let text = format!("file://{address}/uma%20foto.png\r\nfile://localhost{address}/%C3%A7.png\n");
        assert_eq!(files_in_text(&text).as_deref(), Some(&expected[..]));
        for text in ["", "olá", "file:///tmp/a.png\nolá", "file://servidor/a.png", "file:///a%2", "file:///a%zz", "file:///a%ff", "/nao/existe.png"]
        {
            assert_eq!(files_in_text(text), None, "{text:?}");
        }
    }
}
