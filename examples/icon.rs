//! Writes the program's icon, the one its window shows: a PNG for the desktop's launcher, or the
//! `.ico` the Windows program is built with.
//! `cargo run --example icon -- pearl.png` or `… packaging/windows/pearl.ico`

/// The sizes Windows asks for; the largest goes in as a PNG, as `.ico` files do.
const ICO_SIDES: [u32; 5] = [16, 24, 32, 48, 256];

fn png_of(side: u32) -> Vec<u8> {
    let icon = pearl::splash::icon_of_size(side);
    let image = image::RgbaImage::from_raw(icon.width, icon.height, icon.rgba).expect("the icon's size");
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).expect("encode");
    bytes.into_inner()
}

/// A 32-bit picture as an `.ico` holds it: bottom row first, blue first, and a mask after it, unused
/// with an alpha channel but expected.
fn bitmap_of(side: u32) -> Vec<u8> {
    let icon = pearl::splash::icon_of_size(side);
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&side.to_le_bytes());
    out.extend_from_slice(&(side * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for row in icon.rgba.chunks_exact(side as usize * 4).rev() {
        for [red, green, blue, alpha] in row.as_chunks::<4>().0 {
            out.extend_from_slice(&[*blue, *green, *red, *alpha]);
        }
    }
    let mask_row = (side as usize).div_ceil(32) * 4;
    out.resize(out.len() + mask_row * side as usize, 0);
    out
}

fn ico() -> Vec<u8> {
    let pictures: Vec<(u32, Vec<u8>)> = ICO_SIDES.iter().map(|&side| (side, if side == 256 { png_of(side) } else { bitmap_of(side) })).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(pictures.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pictures.len() as u32;
    for (side, bytes) in &pictures {
        // In the directory, 256 is written as 0.
        let side = if *side == 256 { 0 } else { *side as u8 };
        out.extend_from_slice(&[side, side, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += bytes.len() as u32;
    }
    for (_, bytes) in &pictures {
        out.extend_from_slice(bytes);
    }
    out
}

fn main() {
    let out = std::env::args().nth(1).expect("usage: icon out.png | out.ico");
    if out.ends_with(".ico") {
        std::fs::write(&out, ico()).expect("save");
    } else {
        std::fs::write(&out, png_of(128)).expect("save");
    }
}
