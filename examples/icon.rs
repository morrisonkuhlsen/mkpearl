//! Writes the program's icon, the one its window shows, to a PNG for the desktop's launcher.
//! `cargo run --example icon -- pearl.png`

fn main() {
    let out = std::env::args().nth(1).expect("usage: icon out.png");
    let icon = pearl::splash::icon();
    image::RgbaImage::from_raw(icon.width, icon.height, icon.rgba).expect("the icon's size").save(&out).expect("save");
}
