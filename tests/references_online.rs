//! Adding by DOI and checking online, through the window, against a pretend server: every case of a
//! reply, and proof of what leaves the machine and when.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;
use pearl::doi::Servers;

const SECRET: &str = "SEGREDO-DO-DOCUMENTO";

/// Answers as doi.org and Crossref do, and keeps every address asked for.
fn pretend_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut accept = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.to_lowercase().starts_with("accept:") {
                    accept = line.to_lowercase();
                }
                if line.trim().is_empty() {
                    break;
                }
            }
            let path = first.split_whitespace().nth(1).unwrap_or_default().to_string();
            log.lock().unwrap().push(path.clone());
            let bibtex = accept.contains("x-bibtex");
            let (status, body) = match path.as_str() {
                "/10.1000/nova" if bibtex => (
                    "200 OK",
                    "@article{Nova_2020, title={Um artigo novo}, author={Lima, Ana}, year={2020}, DOI={10.1000/nova}, month=May}".to_string(),
                ),
                "/10.1000/lixo" if bibtex => ("200 OK", "<html>nada</html>".to_string()),
                "/10.1000/real" => ("200 OK", r#"{"title":"Artigo com DOI"}"#.to_string()),
                "/10.1000/nova" => ("200 OK", r#"{"title":"Um artigo novo"}"#.to_string()),
                "/10.1000/trocado" => ("200 OK", r#"{"title":"Um trabalho totalmente diferente sobre outro assunto"}"#.to_string()),
                "/10.1000/caiu" | "/10.1000/erro" => ("500 Oops", String::new()),
                p if p.starts_with("/works") && p.contains("Livro%20conhecido") => {
                    ("200 OK", r#"{"message":{"items":[{"title":["Outra coisa"]},{"title":["LIVRO CONHECIDO"]}]}}"#.to_string())
                }
                p if p.starts_with("/works") => ("200 OK", r#"{"message":{"items":[{"title":["Outra coisa"]}]}}"#.to_string()),
                _ => ("404 Not Found", String::new()),
            };
            let mut stream = stream;
            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (address, asked)
}

fn until(h: &mut Harness<'_, App>, what: &str, mut done: impl FnMut(&mut Harness<'_, App>) -> bool) {
    let started = Instant::now();
    while !done(h) {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}: {}", h.state().status());
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn references_are_added_by_doi_and_checked_only_when_asked() {
    let root = std::env::temp_dir().join(format!("pearl-references-online-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this binary has this one test, which sets these before any thread reads them.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let doc = root.join("doc");
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::write(
        doc.join("fontes.bib"),
        r#"
@article{comdoi, title = {Artigo com DOI}, doi = {10.1000/real}, journal = {J}, year = {2020} }
@article{trocado, title = {Artigo que diz ser outro}, doi = {10.1000/trocado}, journal = {J}, year = {2020} }
@article{sumiu, title = {Artigo de DOI inexistente}, doi = {10.1000/sumiu}, journal = {J}, year = {2020} }
@article{fora, title = {Artigo com servidor fora}, doi = {10.1000/caiu}, journal = {J}, year = {2020} }
@book{conhecido, title = {Livro conhecido}, publisher = {P}, year = {2001} }
@book{inventado, title = {Livro inventado}, publisher = {P}, year = {2001} }
"#,
    )
    .unwrap();
    let file = doc.join("doc.typ");
    std::fs::write(&file, format!("{SECRET} @comdoi e @semref\n#bibliography(\"fontes.bib\")\n")).unwrap();

    let (address, asked) = pretend_server();
    let mut app = App::new(Some(file)).unwrap();
    app.use_servers(Servers { doi: address.clone(), crossref: address });
    app.open_side(7);
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).with_max_steps(u64::MAX).with_accessibility_check(false).build_ui_state(
        |ui, app: &mut App| {
            ui.ctx().set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            app.show(ui);
        },
        app,
    );
    until(&mut h, "the list", |h| h.query_by_label("Livro inventado").is_some());

    // Without the network: what's cited with no entry, and what's never cited.
    assert!(h.query_by_label_contains("Citadas sem referência: semref").is_some());
    assert_eq!(h.query_all_by_label("Nunca citada").count(), 5);
    assert!(asked.lock().unwrap().is_empty(), "nothing leaves before it's asked for");

    // What isn't a DOI goes nowhere, and neither does one that's already here.
    h.state_mut().set_doi_input("não é um doi");
    h.step();
    h.get_by_label("Adicionar por DOI").click();
    h.run_steps(2);
    assert!(h.state().status().contains("não parece um DOI"), "{}", h.state().status());
    h.state_mut().set_doi_input("https://doi.org/10.1000/REAL");
    h.step();
    h.get_by_label("Adicionar por DOI").click();
    h.run_steps(2);
    assert!(h.state().status().contains("comdoi"), "{}", h.state().status());
    assert!(asked.lock().unwrap().is_empty());

    // Each way a lookup can go wrong says what happened, and the file stays as it was.
    let before = std::fs::read_to_string(doc.join("fontes.bib")).unwrap();
    for (doi, message) in
        [("10.1000/naoexiste", "não conhece este DOI"), ("10.1000/erro", "Não foi possível buscar"), ("10.1000/lixo", "não devolveu")]
    {
        h.state_mut().set_doi_input(doi);
        h.step();
        h.get_by_label("Adicionar por DOI").click();
        until(&mut h, doi, |h| h.state().status().contains(message));
        assert_eq!(std::fs::read_to_string(doc.join("fontes.bib")).unwrap(), before);
    }

    // A DOI becomes an entry, added to the document's bibliography file, next to what was there.
    h.state_mut().set_doi_input("doi:10.1000/nova");
    h.step();
    h.get_by_label("Adicionar por DOI").click();
    until(&mut h, "the new entry", |h| h.query_by_label("Um artigo novo").is_some());
    let bib = std::fs::read_to_string(doc.join("fontes.bib")).unwrap();
    assert!(bib.starts_with(&before) && bib.contains("Nova_2020"), "{bib}");
    assert!(h.state().status().contains("Nova_2020"), "{}", h.state().status());

    // The check, when asked for, tells every case apart.
    asked.lock().unwrap().clear();
    h.get_by_label("Verificar na internet").click();
    until(&mut h, "the check to end", |h| {
        h.query_by_label("Verificar na internet").is_some() && h.query_all_by_label("Não encontrada na internet").count() == 2
    });
    assert_eq!(h.query_all_by_label("Encontrada na internet").count(), 3, "with its DOI, by title, and the new one");
    assert_eq!(h.query_all_by_label("Não encontrada na internet").count(), 2, "a DOI nobody knows, a title nobody knows");
    assert_eq!(h.query_all_by_label("Não foi possível verificar").count(), 1, "a server failure isn't a missing reference");
    assert!(h.query_by_label_contains("O DOI é de outro título: Um trabalho totalmente").is_some());
    assert!(h.query_by_label_contains("não significa falsa").is_some());

    // Only DOIs and titles went out: nothing of the document's text.
    let sent = asked.lock().unwrap().join("\n");
    assert!(sent.contains("/10.1000/real") && sent.contains("Livro%20conhecido"), "{sent}");
    assert!(!sent.contains(SECRET), "{sent}");
    assert!(!sent.contains("semref"), "{sent}");

    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
