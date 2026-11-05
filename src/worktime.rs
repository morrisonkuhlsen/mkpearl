//! How long a document has been worked on, day by day, across saves and sessions. Kept in the user's
//! data folder, never in the document or a session: it stays on this computer.
//!
//! Only frames with the user's input move the clock: time between two actions counts when they are
//! close together. So nothing redraws to keep count, and a window left open adds nothing.

use crate::history;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Actions further apart than this, in seconds, had a pause between them, which doesn't count.
pub const PAUSE: f64 = 300.0;
/// Seconds between writes while working; what a crash can lose.
const SAVE_EVERY: f64 = 60.0;
/// A file bigger than this is no record of ours: decades of days fit well below it.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Default)]
pub struct WorkTime {
    /// Seconds worked, by local day as `YYYY-MM-DD`.
    days: BTreeMap<String, u64>,
    /// Less than a second not yet in `days`.
    carry: f64,
    /// When the last action was, on the window's clock; none after the window lost its focus.
    last: Option<f64>,
    file: Option<PathBuf>,
    changed: bool,
    saved_at: f64,
}

/// The record of the document at `path`, under `base`: like its history, it doesn't follow a file moved
/// or renamed outside Pearl.
fn file_at(base: &Path, path: &Path) -> PathBuf {
    base.join("pearl").join("worktime").join(format!("{}.txt", history::document_key(path)))
}

/// The days in a record. It's a file on disk, so whatever isn't a line of ours is passed over.
fn parse(text: &str) -> BTreeMap<String, u64> {
    let mut days = BTreeMap::new();
    for (day, seconds) in text.lines().filter_map(|line| line.split_once(' ')) {
        if is_day(day)
            && let Ok(seconds) = seconds.parse::<u64>()
        {
            days.insert(day.to_string(), seconds);
        }
    }
    days
}

fn is_day(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10 && bytes.iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() })
}

impl WorkTime {
    /// The time of the document at `path`, under `base`; empty without a path or a readable record.
    pub fn at(base: Option<&Path>, path: Option<&Path>) -> WorkTime {
        let file = base.zip(path).map(|(base, path)| file_at(base, path));
        let days = file
            .as_deref()
            .filter(|f| std::fs::metadata(f).is_ok_and(|m| m.len() <= MAX_FILE_BYTES))
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map_or_else(BTreeMap::new, |text| parse(&text));
        WorkTime { days, file, ..WorkTime::default() }
    }

    /// The user did something at `time`, in seconds on the window's clock, during the day `today` names.
    pub fn action(&mut self, time: f64, today: impl FnOnce() -> String) {
        let gap = self.last.map_or(0.0, |last| time - last);
        self.last = Some(time);
        // Also what a clock going backwards or a wild number would give.
        if !(gap > 0.0 && gap <= PAUSE) {
            return;
        }
        self.carry += gap;
        let whole = self.carry.floor();
        if whole >= 1.0 {
            self.carry -= whole;
            let day = self.days.entry(today()).or_insert(0);
            *day = day.saturating_add(whole as u64);
            self.changed = true;
        }
    }

    /// The window lost its focus: what comes before the next action is a pause, however short.
    pub fn away(&mut self) {
        self.last = None;
    }

    /// Writes the record now and then, while there's something new.
    pub fn tick(&mut self, time: f64) {
        if self.changed && (time - self.saved_at).abs() >= SAVE_EVERY {
            self.saved_at = time;
            self.save();
        }
    }

    /// Writes the record. A failure leaves it to the next try: this is no reason to trouble someone writing.
    pub fn save(&mut self) {
        let Some(file) = self.file.as_deref().filter(|_| self.changed) else { return };
        let text: String = self.days.iter().map(|(day, seconds)| format!("{day} {seconds}\n")).collect();
        let written =
            file.parent().is_some_and(|dir| history::private_folder(dir).is_ok()) && crate::compiler::write_whole(file, text.as_bytes()).is_ok();
        if written {
            self.changed = false;
        }
    }

    /// The document was saved under another name, or for the first time: the time goes with it, in place
    /// of what a file there had before, whose text has just been replaced too.
    pub fn moved_to(&mut self, base: Option<&Path>, path: &Path) {
        self.file = base.map(|base| file_at(base, path));
        self.changed = !self.days.is_empty();
        self.save();
    }

    pub fn total(&self) -> u64 {
        self.days.values().fold(0, |total, seconds| total.saturating_add(*seconds))
    }

    /// The days worked, the latest first, with their seconds.
    pub fn latest_days(&self, count: usize) -> impl Iterator<Item = (&str, u64)> {
        self.days.iter().rev().take(count).map(|(day, seconds)| (day.as_str(), *seconds))
    }
}

/// The local day `millis` falls on, as a record names it.
pub fn day_of(millis: u64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(i64::try_from(millis).unwrap_or(0))
        .single()
        .map_or_else(|| "1970-01-01".to_string(), |when| when.format("%Y-%m-%d").to_string())
}

/// A record's day as the rest of the program writes dates.
pub fn shown_day(day: &str) -> String {
    match (day.get(8..10), day.get(5..7), day.get(..4)) {
        (Some(d), Some(m), Some(y)) => format!("{d}/{m}/{y}"),
        _ => day.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> String {
        "2026-11-04".to_string()
    }

    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Temp {
            let dir = std::env::temp_dir().join(format!("pearl-worktime-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Temp(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn close_actions_count_and_a_pause_does_not() {
        let mut w = WorkTime::default();
        w.action(10.0, today);
        assert_eq!(w.total(), 0, "the first action only starts the clock");
        w.action(40.0, today);
        assert_eq!(w.total(), 30);
        w.action(40.0 + PAUSE, today);
        assert_eq!(w.total(), 30 + PAUSE as u64, "up to the limit is still working");
        w.action(40.0 + PAUSE + PAUSE + 1.0, today);
        assert_eq!(w.total(), 30 + PAUSE as u64, "past it, nothing");
    }

    #[test]
    fn fractions_of_a_second_add_up() {
        let mut w = WorkTime::default();
        for i in 0..=40 {
            w.action(f64::from(i) * 0.05, today);
        }
        assert_eq!(w.total(), 2);
    }

    #[test]
    fn time_away_from_the_window_does_not_count() {
        let mut w = WorkTime::default();
        w.action(0.0, today);
        w.action(10.0, today);
        w.away();
        w.action(100.0, today);
        assert_eq!(w.total(), 10);
        w.action(105.0, today);
        assert_eq!(w.total(), 15, "and it goes on from the return");
    }

    #[test]
    fn a_strange_clock_adds_nothing() {
        let mut w = WorkTime::default();
        for time in [100.0, 50.0, f64::NAN, f64::INFINITY, -1.0] {
            w.action(time, today);
        }
        assert_eq!(w.total(), 0);
    }

    #[test]
    fn each_day_has_its_own_count_and_the_latest_come_first() {
        let mut w = WorkTime::default();
        w.action(0.0, || "2026-11-03".to_string());
        w.action(60.0, || "2026-11-03".to_string());
        w.action(90.0, today);
        assert_eq!(w.latest_days(5).collect::<Vec<_>>(), [("2026-11-04", 30), ("2026-11-03", 60)]);
        assert_eq!(w.latest_days(1).count(), 1);
        assert_eq!(w.total(), 90);
    }

    #[test]
    fn the_time_comes_back_on_another_day_for_that_document_alone() {
        let temp = Temp::new("back");
        let (one, other) = (temp.0.join("um.typ"), temp.0.join("outro.typ"));
        let mut w = WorkTime::at(Some(&temp.0), Some(&one));
        w.action(0.0, today);
        w.action(90.0, today);
        w.save();
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&one)).total(), 90);
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&other)).total(), 0);
    }

    #[test]
    fn it_is_written_now_and_then_while_working() {
        let temp = Temp::new("tick");
        let file = temp.0.join("um.typ");
        let mut w = WorkTime::at(Some(&temp.0), Some(&file));
        w.action(0.0, today);
        w.action(30.0, today);
        w.tick(30.0);
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&file)).total(), 0, "not yet");
        w.action(70.0, today);
        w.tick(70.0);
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&file)).total(), 70);
    }

    #[test]
    fn saving_under_a_name_takes_the_time_along() {
        let temp = Temp::new("moved");
        let file = temp.0.join("novo.typ");
        std::fs::write(&file, "").unwrap();
        // Before: a document with no file, whose time is only in memory.
        let mut w = WorkTime::at(Some(&temp.0), None);
        w.action(0.0, today);
        w.action(50.0, today);
        w.save();
        w.moved_to(Some(&temp.0), &file);
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&file)).total(), 50);
    }

    #[test]
    fn a_damaged_record_loses_only_its_damaged_lines() {
        let days = parse("2026-11-03 60\nlixo\n2026-11-04 x\n20261104 5\n2026-1-04 5\né 5\n 5\n2026-11-05 18446744073709551616\n2026-11-06 7");
        assert_eq!(days.into_iter().collect::<Vec<_>>(), [("2026-11-03".to_string(), 60), ("2026-11-06".to_string(), 7)]);
    }

    #[test]
    fn huge_or_foreign_files_are_not_read_and_totals_cannot_overflow() {
        let temp = Temp::new("hostile");
        let path = temp.0.join("um.typ");
        let file = file_at(&temp.0, &path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&path)).total(), 0);
        std::fs::write(&file, [0xff, 0xfe, b'\n']).unwrap();
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&path)).total(), 0);
        std::fs::write(&file, format!("2026-11-03 {m}\n2026-11-04 {m}\n", m = u64::MAX)).unwrap();
        let mut w = WorkTime::at(Some(&temp.0), Some(&path));
        assert_eq!(w.total(), u64::MAX);
        w.action(0.0, || "2026-11-04".to_string());
        w.action(5.0, || "2026-11-04".to_string());
        assert_eq!(w.total(), u64::MAX);
    }

    #[test]
    fn a_record_nobody_can_write_is_tried_again() {
        let temp = Temp::new("blocked");
        let path = temp.0.join("um.typ");
        // A file where the folder should go.
        std::fs::write(temp.0.join("pearl"), "").unwrap();
        let mut w = WorkTime::at(Some(&temp.0), Some(&path));
        w.action(0.0, today);
        w.action(5.0, today);
        w.save();
        assert!(w.changed);
        std::fs::remove_file(temp.0.join("pearl")).unwrap();
        w.save();
        assert!(!w.changed);
        assert_eq!(WorkTime::at(Some(&temp.0), Some(&path)).total(), 5);
    }

    #[test]
    fn days_read_as_dates() {
        assert_eq!(shown_day("2026-11-04"), "04/11/2026");
        assert_eq!(shown_day("odd"), "odd");
        assert_eq!(day_of(0).len(), 10);
    }
}
