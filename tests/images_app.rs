//! Pictures through the window, by the same events a real drag and paste send: the file lands in the
//! document's folder, a figure at the caret, and undo and sessions keep working.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{DroppedFile, Event, Key, Modifiers};
use egui_kittest::Harness;
use pearl::app::App;

fn window<'a>(app: App) -> Harness<'a, App> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    )
}

#[derive(Debug)]
struct Dropped(PathBuf);

impl DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn drop_file(h: &mut Harness<'_, App>, path: &Path) {
    h.input_mut().dropped_files.push(Arc::new(Dropped(path.to_path_buf())));
    h.step();
    h.step();
}

/// Long enough for a compile to start, finish and report; then it must have pages and no errors.
fn compiled(h: &mut Harness<'_, App>, what: &str) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2500) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().page_count() > 0 && h.state().error_count() == 0, "{what}: {} errors in {:?}", h.state().error_count(), h.state().text());
}

fn figure(path: &str) -> String {
    format!("#figure(\n  image(\"{path}\"),\n  caption: [Legenda],\n)\n")
}

#[test]
fn pictures_reach_the_document_by_drag_and_by_paste() {
    let root = std::env::temp_dir().join(format!("pearl-images-app-{}", std::process::id()));
    let (project, away) = (root.join("doc"), root.join("away"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&away).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let picture = away.join("uma foto.png");
    image::RgbImage::new(4, 4).save(&picture).unwrap();
    std::fs::write(away.join("notas.txt"), "x").unwrap();

    // With nowhere to put it, it says so and the text stays.
    let mut unsaved = window(App::new(None).unwrap());
    let before = unsaved.state().text().to_string();
    drop_file(&mut unsaved, &picture);
    assert_eq!(unsaved.state().text(), before);
    assert!(unsaved.state().status().contains("Salve o documento"), "{}", unsaved.state().status());
    drop(unsaved);

    let file = project.join("doc.typ");
    std::fs::write(&file, "Texto\n").unwrap();
    let mut h = window(App::new(Some(file.clone())).unwrap());
    compiled(&mut h, "the document alone");

    // A drag: copied in, on a line of its own, and it compiles.
    drop_file(&mut h, &picture);
    assert!(project.join("images/uma foto.png").is_file());
    let dragged = figure("images/uma foto.png");
    assert!(h.state().text().contains(&dragged), "{:?}", h.state().text());
    assert!(h.state().is_dirty());
    compiled(&mut h, "after the drag");
    assert_eq!(std::fs::read(&picture).unwrap(), std::fs::read(project.join("images/uma foto.png")).unwrap());

    // Undo takes it out of the text in one step, and redo brings it back.
    let with_picture = h.state().text().to_string();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.step();
    assert_eq!(h.state().text(), "Texto\n");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.step();
    assert_eq!(h.state().text(), with_picture);

    // What isn't a picture is refused. After the autosave, whose message would take the place.
    let saved = Instant::now();
    while h.state().is_dirty() {
        assert!(saved.elapsed() < Duration::from_secs(15), "autosave");
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    let status_before = h.state().text().to_string();
    drop_file(&mut h, &away.join("notas.txt"));
    assert_eq!(h.state().text(), status_before);
    assert!(h.state().status().contains("Só é possível inserir imagens"), "{}", h.state().status());

    // A picture from the clipboard, as pixels.
    let pixels = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[255, 0, 0, 255]);
    h.event(Event::PasteImage(Arc::new(pixels)));
    h.step();
    h.step();
    let copied = std::fs::read_dir(project.join("images")).unwrap().count();
    assert_eq!(copied, 2, "the pasted picture has a file");
    assert_eq!(h.state().text().matches("#figure(").count(), 2);
    compiled(&mut h, "after pasting pixels");

    // A file copied in a file manager arrives as its address, and isn't typed as text.
    let address = picture.display().to_string().replace('\\', "/").replace(' ', "%20");
    let address = if address.starts_with('/') { address } else { format!("/{address}") };
    h.event(Event::Paste(format!("file://{address}")));
    h.step();
    h.step();
    assert!(!h.state().text().contains("file://"), "{:?}", h.state().text());
    assert_eq!(h.state().text().matches("#figure(").count(), 3);
    assert_eq!(std::fs::read_dir(project.join("images")).unwrap().count(), 2, "the same picture isn't copied twice");

    // Any other text is still text.
    h.event(Event::Paste("olá".to_string()));
    h.step();
    h.step();
    assert!(h.state().text().contains("olá"), "{:?}", h.state().text());
    drop(h);

    // In a session the guests get the text and the picture; a guest can't add one of its own.
    std::fs::write(&file, "Texto\n").unwrap();
    let mut host = App::new(Some(file)).unwrap();
    let link = host.host_session_on("127.0.0.1:0", "Ana").unwrap();
    let mut a = window(host);
    let mut b = window(App::new(None).unwrap());
    let ctx = b.ctx.clone();
    b.state_mut().join_session_link(&ctx, &link, "Beto");
    let until = |a: &mut Harness<'_, App>, b: &mut Harness<'_, App>, what: &str, done: &dyn Fn(&App, &App) -> bool| {
        let started = Instant::now();
        while !done(a.state(), b.state()) {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "{what}: {:?} / {:?}, guest errors {}",
                a.state().text(),
                b.state().text(),
                b.state().error_count()
            );
            a.step();
            b.step();
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    until(&mut a, &mut b, "joined", &|a, b| b.text() == a.text() && b.page_count() > 0 && !a.session_others().is_empty());
    drop_file(&mut a, &picture);
    assert!(a.state().text().contains(&dragged));
    until(&mut a, &mut b, "the picture reaches the guest", &|a, b| b.text() == a.text());
    compiled(&mut a, "the host");
    compiled(&mut b, "the guest, with the picture sent to it");

    let shared = b.state().text().to_string();
    drop_file(&mut b, &picture);
    assert_eq!(b.state().text(), shared);
    assert!(b.state().status().contains("Só quem hospeda"), "{}", b.state().status());

    // The host's undo takes back its own insertion, for both.
    a.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    until(&mut a, &mut b, "the undo reaches the guest", &|a, b| a.text() == "Texto\n" && b.text() == "Texto\n");

    a.state_mut().leave_session();
    drop(a);
    drop(b);
    std::fs::remove_dir_all(root).unwrap();
}
