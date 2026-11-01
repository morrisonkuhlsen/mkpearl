//! Two sessions talking over a real, encrypted connection on this machine.

use std::time::{Duration, Instant};

use pearl::collab::{Change, Session};

fn host(text: &str) -> Session {
    Session::host_on("127.0.0.1:0", text, "Ana", "tese.typ").unwrap()
}

/// Polls both sides until `done` says so, syncing nothing by itself.
fn until(a: &mut Session, b: &mut Session, mut done: impl FnMut(&Session, &Session) -> bool) -> Vec<Change> {
    let started = Instant::now();
    let mut seen = Vec::new();
    while !done(a, b) {
        assert!(started.elapsed() < Duration::from_secs(20), "timed out; a={:?} b={:?} seen={seen:?}", a.text(), b.text());
        seen.extend(a.poll());
        seen.extend(b.poll());
        std::thread::sleep(Duration::from_millis(5));
    }
    seen
}

fn pair(text: &str) -> (Session, Session) {
    let mut a = host(text);
    let mut b = Session::join(a.share_link().unwrap(), "Beto").unwrap();
    until(&mut a, &mut b, |_, b| !b.is_waiting());
    (a, b)
}

#[test]
fn a_guest_receives_the_document_with_its_accents() {
    let (a, b) = pair("ação 日本語 🙂");
    assert_eq!(b.text(), "ação 日本語 🙂");
    assert_eq!(a.others(), ["Beto"]);
    assert!(b.guest_path().unwrap().ends_with("tese.typ"));
}

#[test]
fn edits_travel_both_ways_and_merge() {
    let (mut a, mut b) = pair("meio");
    a.sync_local("início meio");
    b.sync_local("meio fim");
    until(&mut a, &mut b, |a, b| a.text() == b.text() && a.text().len() > 12);
    assert_eq!(a.text(), "início meio fim");
}

#[test]
fn three_people_see_the_same_text_through_the_host() {
    let mut a = host("x");
    let link = a.share_link().unwrap().to_string();
    let mut b = Session::join(&link, "Beto").unwrap();
    let mut c = Session::join(&link, "Cris").unwrap();
    let started = Instant::now();
    while b.is_waiting() || c.is_waiting() {
        assert!(started.elapsed() < Duration::from_secs(20));
        a.poll();
        b.poll();
        c.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    b.sync_local("xB");
    let started = Instant::now();
    while c.text() != "xB" || a.text() != "xB" {
        assert!(started.elapsed() < Duration::from_secs(20), "{:?}", c.text());
        a.poll();
        b.poll();
        c.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn carets_show_with_names_and_follow_the_text() {
    let (mut a, mut b) = pair("olá mundo");
    let at = "olá ".len();
    a.send_cursor(at, at);
    until(&mut a, &mut b, |_, b| !b.cursors().is_empty());
    let seen = b.cursors();
    assert_eq!((seen[0].name.as_str(), seen[0].byte), ("Ana", at));
    b.sync_local("ei, olá mundo");
    until(&mut a, &mut b, |a, _| a.text() == "ei, olá mundo");
    let moved = b.cursors()[0].byte;
    assert_eq!(b.text().get(moved..), Some("mundo"));
}

#[test]
fn a_wrong_key_never_gets_in() {
    let mut a = host("segredo");
    let link = a.share_link().unwrap().to_string();
    let (address, _) = pearl::collab::wire::parse_link(&link).unwrap();
    let wrong = pearl::collab::wire::format_link(&address, &[9; 32]);
    let mut b = Session::join(&wrong, "Intruso").unwrap();
    let seen = until(&mut a, &mut b, |_, _| false_after(Duration::from_millis(1500)));
    assert!(seen.iter().any(|c| matches!(c, Change::Ended(_))), "{seen:?}");
    assert!(a.others().is_empty());
    assert_eq!(b.text(), "");
    assert!(b.is_waiting());
}

/// True once `wait` has passed since the first call on this thread.
fn false_after(wait: Duration) -> bool {
    thread_local!(static START: Instant = Instant::now());
    START.with(|s| s.elapsed() >= wait)
}

#[test]
fn a_stranger_sending_garbage_does_not_disturb_the_session() {
    use std::io::Write;
    let (mut a, mut b) = pair("base");
    let (address, _) = pearl::collab::wire::parse_link(a.share_link().unwrap()).unwrap();
    let mut stranger = std::net::TcpStream::connect(address).unwrap();
    stranger.write_all(&[0xFF; 4096]).unwrap();
    drop(stranger);
    a.sync_local("base ok");
    until(&mut a, &mut b, |_, b| b.text() == "base ok");
    assert_eq!(a.others(), ["Beto"]);
}

#[test]
fn when_the_host_leaves_the_guest_is_told_and_keeps_the_text() {
    let (a, mut b) = pair("guardado");
    drop(a);
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < Duration::from_secs(20));
        if b.poll().iter().any(|c| matches!(c, Change::Ended(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(b.text(), "guardado");
}

#[test]
fn when_a_guest_leaves_the_host_is_told_and_its_caret_goes() {
    let (mut a, mut b) = pair("x");
    b.send_cursor(0, 0);
    until(&mut a, &mut b, |a, _| !a.cursors().is_empty());
    drop(b);
    let started = Instant::now();
    while !a.others().is_empty() || !a.cursors().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(20));
        a.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_files_a_document_uses_reach_the_guest_only_in_its_session_folder() {
    let dir = std::env::temp_dir().join(format!("pearl-test-host-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let picture = dir.join("foto.png");
    std::fs::write(&picture, b"pretend picture").unwrap();
    let (mut a, mut b) = pair("x");
    a.share_files(vec![("fig/foto.png".to_string(), picture)]);
    let seen = until(&mut a, &mut b, |_, b| b.guest_path().unwrap().with_file_name("fig").join("foto.png").exists());
    let _ = seen;
    let folder = b.guest_path().unwrap().parent().unwrap().to_path_buf();
    assert_eq!(std::fs::read(folder.join("fig/foto.png")).unwrap(), b"pretend picture");
    drop(b);
    assert!(!folder.exists(), "the session's folder goes with the session");
    std::fs::remove_dir_all(dir).unwrap();
}
