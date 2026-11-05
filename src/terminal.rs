//! A terminal beside the document, for running a coding agent such as Claude Code or Codex in its
//! folder. `alacritty_terminal` keeps the screen and talks to the pseudoterminal on a thread of its
//! own; this module starts it, turns keys into what programs expect, and takes a snapshot of the
//! screen for the window to paint.
//!
//! What a program writes is hostile input: the emulator reads it, the scrollback is capped, and
//! nothing here indexes the screen without checking. The terminal is local only: nothing from a
//! session reaches it and its output goes nowhere else. A program starts only from a click.

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use egui::{Color32, Key, Modifiers};

/// Lines kept above the screen.
pub const SCROLLBACK: usize = 5_000;
/// The most columns and lines a terminal is given, whatever the window's size.
const MAX_COLUMNS: usize = 1_000;
const MAX_LINES: usize = 500;
/// A title longer than this is cut.
const MAX_TITLE: usize = 200;

/// What runs in the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Shell,
    Claude,
    Codex,
}

impl Program {
    /// The command run through the login shell; none for the shell itself.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Program::Shell => None,
            Program::Claude => Some("claude"),
            Program::Codex => Some("codex"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Program::Shell => crate::i18n::tx("shell"),
            Program::Claude => "Claude Code",
            Program::Codex => "Codex",
        }
    }
}

/// Carries the emulator's news to the window, and wakes it.
#[derive(Clone)]
struct Listener {
    events: Sender<TermEvent>,
    waker: egui::Context,
}

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        if !matches!(event, TermEvent::Wakeup | TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange) {
            let _ = self.events.send(event);
        }
        self.waker.request_repaint();
    }
}

/// A running terminal. Dropping it ends the program, and the pseudoterminal with it.
pub struct Terminal {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    events: Receiver<TermEvent>,
    columns: usize,
    lines: usize,
    cell: (u16, u16),
    pub program: Program,
    /// Set by the program, shown in the pane's header.
    pub title: Option<String>,
    /// Set when the program has ended, with its exit code when there is one.
    pub ended: Option<Option<i32>>,
    focused: bool,
    /// The program's process, to end it if it won't hang up.
    #[cfg(unix)]
    pid: Option<u32>,
}

fn window_size(columns: usize, lines: usize, cell: (u16, u16)) -> WindowSize {
    WindowSize {
        num_cols: u16::try_from(columns).unwrap_or(u16::MAX),
        num_lines: u16::try_from(lines).unwrap_or(u16::MAX),
        cell_width: cell.0,
        cell_height: cell.1,
    }
}

/// How the program is started: the command itself goes through the user's login shell, so the
/// `PATH` an app started from the system menu lacks is there, and then through `sh`, so the check
/// for a missing program reads the same in every shell. A shell stays when the program ends.
fn shell_for(command: Option<&str>) -> Option<tty::Shell> {
    command?;
    #[cfg(windows)]
    {
        let script = "if (Get-Command $env:PEARL_PROGRAM -ErrorAction SilentlyContinue) { if ($env:PEARL_PROMPT) { & $env:PEARL_PROGRAM \
            --append-system-prompt $env:PEARL_PROMPT } else { & $env:PEARL_PROGRAM } } else { Write-Host $env:PEARL_MISSING }";
        Some(tty::Shell::new("powershell.exe".into(), vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), script.into()]))
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "/bin/sh".into());
        Some(tty::Shell::new(shell, vec!["-l".into(), "-i".into(), "-c".into(), "exec /bin/sh -c \"$PEARL_SCRIPT\"".into()]))
    }
}

/// What `sh` runs: the program if it's there, told of the document when there's something to tell,
/// a message if it isn't there, and then the user's shell.
const SCRIPT: &str = "if command -v \"$PEARL_PROGRAM\" >/dev/null 2>&1; then if [ -n \"$PEARL_PROMPT\" ]; then \"$PEARL_PROGRAM\" \
    --append-system-prompt \"$PEARL_PROMPT\"; else \"$PEARL_PROGRAM\"; fi; else printf '%s\\n' \"$PEARL_MISSING\"; fi; exec \"${SHELL:-/bin/sh}\"";

/// What Claude Code is told of the document open beside it, so that "write it in Pearl" means that
/// file, edited in place, and not a new one. Meant for the model, so not translated.
pub fn document_prompt(document: &Path) -> String {
    format!(
        "You are running in a terminal inside Pearl, a Typst document editor. The document open in Pearl is {}. \
         When the user asks you to write, change or fix the document, or to write something \"in Pearl\", edit that file in place: \
         Pearl shows every save on its pages at once and keeps the earlier text in its version history. \
         Don't create a new file or open another Pearl window unless the user asks for one. \
         Pearl compiles the document itself; the `typst` program may not be installed.",
        document.display()
    )
}

/// What to start, and where.
#[derive(Clone, Copy)]
pub struct Launch<'a> {
    pub program: Program,
    /// None for the shell.
    pub command: Option<&'a str>,
    pub folder: &'a Path,
    /// The document open in Pearl, which the program is told of.
    pub document: Option<&'a Path>,
    /// What's said if the program isn't installed.
    pub missing: &'a str,
}

impl Terminal {
    /// Starts what `launch` says, `columns` × `lines` of `cell` pixels.
    pub fn start(ctx: &egui::Context, launch: &Launch<'_>, (columns, lines): (usize, usize), cell: (u16, u16)) -> std::io::Result<Terminal> {
        let Launch { program, command, folder, document, missing } = *launch;
        let columns = columns.clamp(2, MAX_COLUMNS);
        let lines = lines.clamp(1, MAX_LINES);
        let (to_window, events) = channel();
        let listener = Listener { events: to_window, waker: ctx.clone() };
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &TermSize::new(columns, lines), listener.clone())));
        let mut env = std::collections::HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        env.insert("TERM_PROGRAM".to_string(), "Pearl".to_string());
        if let Some(document) = document {
            env.insert("PEARL_DOCUMENT".to_string(), document.display().to_string());
            // Only Claude Code takes this option; others learn of the document from the variable.
            if program == Program::Claude {
                env.insert("PEARL_PROMPT".to_string(), document_prompt(document));
            }
        }
        if let Some(command) = command {
            env.insert("PEARL_PROGRAM".to_string(), command.to_string());
            env.insert("PEARL_MISSING".to_string(), missing.to_string());
            env.insert("PEARL_SCRIPT".to_string(), SCRIPT.to_string());
        }
        // Windows has one more field, set below.
        #[allow(clippy::needless_update)]
        let options = tty::Options {
            shell: shell_for(command),
            working_directory: Some(folder.to_path_buf()),
            drain_on_exit: false,
            env,
            ..Default::default()
        };
        #[cfg(windows)]
        let options = tty::Options { escape_args: true, ..options };
        let pty = tty::new(&options, window_size(columns, lines, cell), 0)?;
        #[cfg(unix)]
        let pid = {
            no_flow_control(pty.file());
            Some(pty.child().id())
        };
        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)?;
        let sender = event_loop.channel();
        let _thread = event_loop.spawn();
        Ok(Terminal {
            term,
            sender,
            events,
            columns,
            lines,
            cell,
            program,
            title: None,
            ended: None,
            focused: false,
            #[cfg(unix)]
            pid,
        })
    }

    /// Sends bytes to the program, as typed.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let bytes = bytes.into();
        if !bytes.is_empty() && self.ended.is_none() {
            let _ = self.sender.send(Msg::Input(bytes));
        }
    }

    /// Typing brings the screen back from the scrollback.
    pub fn input(&self, bytes: Vec<u8>) {
        self.term.lock().scroll_display(Scroll::Bottom);
        self.write(bytes);
    }

    /// Takes in what the emulator asked for since the last frame.
    pub fn poll(&mut self, ctx: &egui::Context, palette: &Palette) {
        let events: Vec<TermEvent> = self.events.try_iter().collect();
        for event in events {
            match event {
                TermEvent::PtyWrite(text) => self.write(text.into_bytes()),
                TermEvent::ClipboardStore(_, text) => ctx.copy_text(text),
                // A program doesn't get to read the clipboard: it may hold anything.
                TermEvent::ClipboardLoad(..) => {}
                TermEvent::ColorRequest(index, reply) => {
                    let colours = *self.term.lock().colors();
                    let c = palette.by_index(&colours, index);
                    self.write(reply(Rgb { r: c.r(), g: c.g(), b: c.b() }).into_bytes());
                }
                TermEvent::TextAreaSizeRequest(reply) => self.write(reply(window_size(self.columns, self.lines, self.cell)).into_bytes()),
                TermEvent::Title(title) => self.title = Some(title.chars().filter(|c| !c.is_control()).take(MAX_TITLE).collect()),
                TermEvent::ResetTitle => self.title = None,
                TermEvent::ChildExit(status) => self.ended = Some(status.code()),
                TermEvent::Exit => self.ended = self.ended.or(Some(None)),
                _ => {}
            }
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.columns, self.lines)
    }

    /// Fits the screen to `columns` × `lines`.
    pub fn resize(&mut self, columns: usize, lines: usize, cell: (u16, u16)) {
        let columns = columns.clamp(2, MAX_COLUMNS);
        let lines = lines.clamp(1, MAX_LINES);
        if (columns, lines, cell) == (self.columns, self.lines, self.cell) {
            return;
        }
        self.columns = columns;
        self.lines = lines;
        self.cell = cell;
        self.term.lock().resize(TermSize::new(columns, lines));
        let _ = self.sender.send(Msg::Resize(window_size(columns, lines, cell)));
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Tells the program the terminal gained or lost the keyboard, when it asked to know.
    pub fn set_focused(&mut self, focused: bool) {
        if focused != self.focused {
            self.focused = focused;
            if self.mode().contains(TermMode::FOCUS_IN_OUT) {
                self.write(if focused { &b"\x1b[I"[..] } else { &b"\x1b[O"[..] });
            }
        }
    }

    /// Pastes text, marked as pasted when the program asked for that.
    pub fn paste(&self, text: &str) {
        let bracketed = self.mode().contains(TermMode::BRACKETED_PASTE);
        self.input(paste_bytes(text, bracketed));
    }

    /// Scrolls by `lines` (up is positive), or tells the program, when it takes the wheel itself.
    pub fn scroll(&self, lines: i32, at: (usize, usize)) {
        if lines == 0 {
            return;
        }
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
            let button = if lines > 0 { 64 } else { 65 };
            let one = format!("\x1b[<{button};{};{}M", at.0.saturating_add(1), at.1.saturating_add(1));
            self.write(one.repeat(lines.unsigned_abs().min(20) as usize).into_bytes());
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            let arrow: &[u8] = match (lines > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            self.write(arrow.repeat(lines.unsigned_abs().min(20) as usize));
        } else {
            self.term.lock().scroll_display(Scroll::Delta(lines));
        }
    }

    /// The grid's place of a cell on screen.
    fn point(&self, (column, line): (usize, usize)) -> Point {
        let term = self.term.lock();
        let offset = i32::try_from(term.grid().display_offset()).unwrap_or(0);
        let line = i32::try_from(line.min(self.lines.saturating_sub(1))).unwrap_or(0) - offset;
        Point::new(Line(line), Column(column.min(self.columns.saturating_sub(1))))
    }

    /// Starts a selection at a cell: by character, by word (two clicks) or by line (three).
    pub fn select_from(&self, at: (usize, usize), right_half: bool, clicks: u8) {
        let point = self.point(at);
        let kind = match clicks {
            2 => SelectionType::Semantic,
            3.. => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        let side = if right_half { Side::Right } else { Side::Left };
        self.term.lock().selection = Some(Selection::new(kind, point, side));
    }

    pub fn select_to(&self, at: (usize, usize), right_half: bool) {
        let point = self.point(at);
        let side = if right_half { Side::Right } else { Side::Left };
        if let Some(selection) = self.term.lock().selection.as_mut() {
            selection.update(point, side);
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// What's selected, if anything.
    pub fn selected_text(&self) -> Option<String> {
        self.term.lock().selection_to_string().filter(|s| !s.is_empty())
    }

    /// The link under a cell, if it's one a browser may open.
    pub fn link_at(&self, at: (usize, usize)) -> Option<String> {
        let point = self.point(at);
        let term = self.term.lock();
        let grid = term.grid();
        let inside =
            point.line.0 >= -i32::try_from(grid.history_size()).unwrap_or(0) && point.line.0 < i32::try_from(grid.screen_lines()).unwrap_or(0);
        if !inside || point.column.0 >= grid.columns() {
            return None;
        }
        let uri = grid[point].hyperlink()?.uri().to_string();
        safe_link(&uri).then_some(uri)
    }

    /// What there is to paint now.
    pub fn snapshot(&self, palette: &Palette) -> Screen {
        snapshot(&self.term.lock(), palette, self.focused)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // The emulator's thread then drops the pseudoterminal, which hangs up on the program.
        let _ = self.sender.send(Msg::Shutdown);
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            end_if_it_stays(pid);
        }
    }
}

/// Ctrl+S would stop all output until Ctrl+Q, and a shell whose output is stopped doesn't end when
/// hung up on. Nobody means it in a pane beside a document.
#[cfg(unix)]
fn no_flow_control(file: &std::fs::File) {
    use rustix::termios::{InputModes, OptionalActions, tcgetattr, tcsetattr};
    if let Ok(mut termios) = tcgetattr(file) {
        termios.input_modes.remove(InputModes::IXON);
        let _ = tcsetattr(file, OptionalActions::Now, &termios);
    }
}

/// How long a program has to end after being hung up on.
#[cfg(unix)]
const GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Kills the program and its process group if it's still there after `GRACE`, so it neither stays
/// behind nor holds the emulator's thread, which waits for it.
#[cfg(unix)]
fn end_if_it_stays(pid: u32) {
    use rustix::process::{Pid, Signal, kill_process, kill_process_group, test_kill_process};
    let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) else { return };
    let _ = std::thread::Builder::new().name("terminal-end".into()).spawn(move || {
        let started = std::time::Instant::now();
        while started.elapsed() < GRACE {
            if test_kill_process(pid).is_err() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // Only our own child: a number freed and taken by another process is left alone.
        if !is_our_child(pid) {
            return;
        }
        let _ = kill_process_group(pid, Signal::KILL);
        let _ = kill_process(pid, Signal::KILL);
    });
}

#[cfg(unix)]
fn is_our_child(pid: rustix::process::Pid) -> bool {
    match std::fs::read_to_string(format!("/proc/{}/stat", pid.as_raw_nonzero())) {
        // The parent is the field after the name, which ends at the last ')'.
        Ok(stat) => stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1)?.parse::<u32>().ok())
            .is_some_and(|parent| parent == std::process::id()),
        // Without /proc, the program leads its own session, as it was started.
        Err(_) => rustix::process::getsid(Some(pid)).is_ok_and(|sid| sid == pid),
    }
}

/// Only web links open from the terminal: a program could name any file or scheme.
pub fn safe_link(uri: &str) -> bool {
    let lower = uri.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && uri.len() <= 2048 && !uri.chars().any(char::is_control)
}

/// What a paste sends. Marked, it can't end its own marking early; unmarked, new lines are Enter.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let clean: String = text.replace("\x1b[201~", "").chars().filter(|&c| c != '\x1b').collect();
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(clean.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// The xterm number for the modifiers held, in sequences such as `ESC [ 1 ; 5 A`.
fn modifier_code(m: Modifiers) -> u8 {
    1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl)
}

/// What a key sends, in the terminal's present mode. None for keys left to the window.
pub fn key_bytes(key: Key, m: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let held = m.shift || m.alt || m.ctrl;
    let code = modifier_code(m);
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let cursor = |letter: char| -> Vec<u8> {
        if held {
            format!("\x1b[1;{code}{letter}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> { if held { format!("\x1b[{n};{code}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() } };
    let function =
        |letter: char| -> Vec<u8> { if held { format!("\x1b[1;{code}{letter}").into_bytes() } else { format!("\x1bO{letter}").into_bytes() } };
    let alt = |bytes: &[u8]| -> Vec<u8> {
        let mut out = if m.alt { vec![0x1b] } else { Vec::new() };
        out.extend_from_slice(bytes);
        out
    };
    // On macOS the Command key belongs to the window.
    if m.mac_cmd {
        return None;
    }
    Some(match key {
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F1 => function('P'),
        Key::F2 => function('Q'),
        Key::F3 => function('R'),
        Key::F4 => function('S'),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        // Shift+Enter as Alt+Enter, which agents read as a new line in what's being written.
        Key::Enter if m.shift || m.alt => b"\x1b\r".to_vec(),
        Key::Enter => b"\r".to_vec(),
        Key::Tab if m.shift => b"\x1b[Z".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Backspace if m.ctrl => alt(b"\x08"),
        Key::Backspace => alt(b"\x7f"),
        Key::Escape => b"\x1b".to_vec(),
        Key::Space if m.ctrl => alt(b"\x00"),
        _ if m.ctrl => alt(&[control_byte(key)?]),
        // Alt with a letter or digit: the letter, after Escape. Text alone comes as text.
        _ if m.alt && !m.shift => {
            let name = key.name();
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => alt(c.to_ascii_lowercase().to_string().as_bytes()),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// What Ctrl with a key sends, as terminals have always sent it.
fn control_byte(key: Key) -> Option<u8> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase() as u8 - b'a' + 1),
        _ => match key {
            Key::OpenBracket => Some(0x1b),
            Key::Backslash => Some(0x1c),
            Key::CloseBracket => Some(0x1d),
            Key::Slash | Key::Minus => Some(0x1f),
            Key::Num2 => Some(0x00),
            _ => None,
        },
    }
}

/// The colours a program can name: the 256 of xterm, then the foreground, background, cursor and
/// the dim ones, after the theme.
#[derive(Clone)]
pub struct Palette {
    colours: Vec<Color32>,
}

const COLOURS: usize = 269;

impl Palette {
    pub fn new(dark: bool, foreground: Color32, background: Color32, cursor: Color32) -> Palette {
        // Original values, chosen to read on the theme's own background.
        let named: [u32; 16] = if dark {
            [
                0x2a2d33, 0xe06c6c, 0x8cc47a, 0xd9b45a, 0x6fa0e0, 0xc08ad8, 0x5fbfbf, 0xc8ccd2, 0x6b7079, 0xff8a8a, 0xa6de92, 0xf0cd73, 0x8db8f5,
                0xd7a6ee, 0x7fd8d8, 0xf2f4f7,
            ]
        } else {
            [
                0x2b2e33, 0xb83232, 0x3e7d2a, 0x8a6400, 0x2a5fb0, 0x8a3fa8, 0x1f7f80, 0xd5d8dd, 0x6b7079, 0xd14545, 0x4f9636, 0xa87b00, 0x3b76d0,
                0xa556c4, 0x2a9798, 0xf4f5f7,
            ]
        };
        let rgb = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        let mut colours: Vec<Color32> = named.iter().map(|&v| rgb(v)).collect();
        let level = |i: u8| if i == 0 { 0 } else { 55 + 40 * i };
        for r in 0..6u8 {
            for g in 0..6u8 {
                for b in 0..6u8 {
                    colours.push(Color32::from_rgb(level(r), level(g), level(b)));
                }
            }
        }
        for i in 0..24u8 {
            let v = 8 + 10 * i;
            colours.push(Color32::from_rgb(v, v, v));
        }
        colours.push(foreground);
        colours.push(background);
        colours.push(cursor);
        for i in 0..8 {
            colours.push(dim(colours.get(i).copied().unwrap_or(foreground)));
        }
        colours.push(foreground);
        colours.push(dim(foreground));
        Palette { colours }
    }

    pub fn foreground(&self) -> Color32 {
        self.colours.get(NamedColor::Foreground as usize).copied().unwrap_or(Color32::GRAY)
    }

    pub fn background(&self) -> Color32 {
        self.colours.get(NamedColor::Background as usize).copied().unwrap_or(Color32::BLACK)
    }

    /// A cell's colour: what the program set for that number, or else the palette's.
    fn resolve(&self, set: &alacritty_terminal::term::color::Colors, colour: Color, fallback: usize) -> Color32 {
        let index = match colour {
            Color::Spec(rgb) => return Color32::from_rgb(rgb.r, rgb.g, rgb.b),
            Color::Indexed(i) => usize::from(i),
            Color::Named(n) => n as usize,
        };
        self.by_index(set, if index < COLOURS { index } else { fallback })
    }

    fn by_index(&self, set: &alacritty_terminal::term::color::Colors, index: usize) -> Color32 {
        let index = index.min(COLOURS - 1);
        match set[index] {
            Some(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
            None => self.colours.get(index).copied().unwrap_or(Color32::GRAY),
        }
    }
}

fn dim(c: Color32) -> Color32 {
    Color32::from_rgb((u16::from(c.r()) * 2 / 3) as u8, (u16::from(c.g()) * 2 / 3) as u8, (u16::from(c.b()) * 2 / 3) as u8)
}

/// One cell to paint.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub column: usize,
    pub line: usize,
    /// Two columns wide.
    pub wide: bool,
    pub text: String,
    pub fg: Color32,
    /// None where the background shows.
    pub bg: Option<Color32>,
    pub bold: bool,
    pub underline: bool,
    pub strike: bool,
    pub link: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caret {
    Block,
    Hollow,
    Beam,
    Underline,
}

/// The screen as it is, ready to paint.
#[derive(Clone, Debug, Default)]
pub struct Screen {
    pub cells: Vec<Drawn>,
    pub caret: Option<(usize, usize, Caret)>,
    /// Lines scrolled back from the bottom, and how many there are.
    pub scrolled: usize,
    pub history: usize,
}

/// What `term` shows. A cell outside the screen, as hostile output might try, is left out.
pub fn snapshot<T: EventListener>(term: &Term<T>, palette: &Palette, focused: bool) -> Screen {
    let content = term.renderable_content();
    let offset = content.display_offset;
    let lines = term.screen_lines();
    let selection_bg = palette.colours.get(4).copied().unwrap_or(Color32::BLUE).gamma_multiply(0.55);
    let mut cells = Vec::new();
    for indexed in content.display_iter {
        let cell = indexed.cell;
        if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }
        let Some(line) = usize::try_from(i64::from(indexed.point.line.0) + offset as i64).ok().filter(|l| *l < lines) else { continue };
        let mut fg = palette.resolve(content.colors, cell.fg, NamedColor::Foreground as usize);
        let mut bg = palette.resolve(content.colors, cell.bg, NamedColor::Background as usize);
        if cell.flags.contains(Flags::DIM) {
            fg = dim(fg);
        }
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let selected = content.selection.is_some_and(|s| s.contains(indexed.point));
        let bg = if selected { Some(selection_bg) } else { (bg != palette.background()).then_some(bg) };
        let mut text = String::new();
        if !cell.flags.contains(Flags::HIDDEN) && cell.c != ' ' && cell.c != '\t' && !cell.c.is_control() {
            text.push(cell.c);
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra.iter().take(8));
            }
        }
        if text.is_empty() && bg.is_none() && !cell.flags.intersects(Flags::ALL_UNDERLINES | Flags::STRIKEOUT) {
            continue;
        }
        cells.push(Drawn {
            column: indexed.point.column.0,
            line,
            wide: cell.flags.contains(Flags::WIDE_CHAR),
            text,
            fg,
            bg,
            bold: cell.flags.contains(Flags::BOLD),
            underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
            strike: cell.flags.contains(Flags::STRIKEOUT),
            link: cell.hyperlink().is_some(),
        });
    }
    let cursor = content.cursor;
    let caret = usize::try_from(i64::from(cursor.point.line.0) + offset as i64).ok().filter(|l| *l < lines).and_then(|line| {
        let shape = match cursor.shape {
            CursorShape::Hidden => return None,
            _ if !focused => Caret::Hollow,
            CursorShape::Block => Caret::Block,
            CursorShape::HollowBlock => Caret::Hollow,
            CursorShape::Beam => Caret::Beam,
            CursorShape::Underline => Caret::Underline,
        };
        Some((cursor.point.column.0, line, shape))
    });
    Screen { cells, caret, scrolled: offset, history: term.grid().history_size() }
}

/// The text of the screen, line by line, as tests read it.
pub fn screen_text(screen: &Screen) -> String {
    let lines = screen.cells.iter().map(|c| c.line).max().map_or(0, |l| l + 1);
    let mut rows = vec![(String::new(), 0usize); lines];
    for cell in &screen.cells {
        if let Some((row, at)) = rows.get_mut(cell.line) {
            if cell.column > *at {
                row.extend(std::iter::repeat_n(' ', cell.column - *at));
                *at = cell.column;
            }
            row.push_str(&cell.text);
            *at += if cell.wide { 2 } else { 1 };
        }
    }
    rows.into_iter().map(|(row, _)| row).collect::<Vec<_>>().join("\n")
}

impl Terminal {
    /// The text on screen now, for tests and for reading what a program said.
    pub fn text(&self, palette: &Palette) -> String {
        screen_text(&self.snapshot(palette))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::vte::ansi::Processor;

    fn palette() -> Palette {
        Palette::new(true, Color32::WHITE, Color32::BLACK, Color32::RED)
    }

    fn fed(bytes: &[u8], columns: usize, lines: usize) -> Term<VoidListener> {
        let mut term = Term::new(Config { scrolling_history: SCROLLBACK, ..Config::default() }, &TermSize::new(columns, lines), VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        term
    }

    #[test]
    fn keys_send_what_programs_expect() {
        let none = Modifiers::NONE;
        let mode = TermMode::empty();
        assert_eq!(key_bytes(Key::Enter, none, mode), Some(b"\r".to_vec()));
        assert_eq!(key_bytes(Key::Enter, Modifiers::SHIFT, mode), Some(b"\x1b\r".to_vec()));
        assert_eq!(key_bytes(Key::Escape, none, mode), Some(b"\x1b".to_vec()));
        assert_eq!(key_bytes(Key::Tab, Modifiers::SHIFT, mode), Some(b"\x1b[Z".to_vec()));
        assert_eq!(key_bytes(Key::ArrowUp, none, mode), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(Key::ArrowUp, none, TermMode::APP_CURSOR), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_bytes(Key::ArrowLeft, Modifiers::CTRL, mode), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(key_bytes(Key::C, Modifiers::CTRL, mode), Some(vec![3]));
        assert_eq!(key_bytes(Key::D, Modifiers::CTRL, mode), Some(vec![4]));
        assert_eq!(key_bytes(Key::B, Modifiers::ALT, mode), Some(b"\x1bb".to_vec()));
        assert_eq!(key_bytes(Key::Backspace, none, mode), Some(vec![0x7f]));
        assert_eq!(key_bytes(Key::Delete, none, mode), Some(b"\x1b[3~".to_vec()));
        assert_eq!(key_bytes(Key::F5, none, mode), Some(b"\x1b[15~".to_vec()));
        assert_eq!(key_bytes(Key::A, none, mode), None, "text comes as text");
        assert_eq!(key_bytes(Key::C, Modifiers::MAC_CMD, mode), None, "Command is the window's");
    }

    #[test]
    fn a_paste_cant_end_its_own_marking() {
        assert_eq!(paste_bytes("a\nb", false), b"a\rb".to_vec());
        let marked = paste_bytes("x\x1b[201~rm -rf ~\x1b", true);
        assert_eq!(marked, b"\x1b[200~xrm -rf ~\x1b[201~".to_vec());
    }

    #[test]
    fn only_web_links_open() {
        assert!(safe_link("https://example.org/a"));
        assert!(safe_link("HTTP://example.org"));
        assert!(!safe_link("file:///etc/passwd"));
        assert!(!safe_link("javascript:alert(1)"));
        assert!(!safe_link("https://example.org/\nx"));
    }

    #[test]
    fn colours_wide_characters_and_the_caret_are_read() {
        let term = fed("\x1b[31mred\x1b[0m é 漢\x1b[7mx".as_bytes(), 20, 3);
        let screen = snapshot(&term, &palette(), true);
        assert_eq!(screen_text(&screen), "red é 漢x");
        let red = screen.cells.iter().find(|c| c.text == "r").unwrap();
        assert_eq!(red.fg, palette().colours[1]);
        let wide = screen.cells.iter().find(|c| c.text == "漢").unwrap();
        assert!(wide.wide);
        let inverse = screen.cells.iter().find(|c| c.text == "x").unwrap();
        assert_eq!(inverse.bg, Some(Color32::WHITE));
        assert_eq!(screen.caret, Some((9, 0, Caret::Block)), "after the wide character's two columns");
    }

    #[test]
    fn the_scrollback_is_capped() {
        let many: String = (0..SCROLLBACK + 500).map(|i| format!("{i}\r\n")).collect();
        let term = fed(many.as_bytes(), 10, 5);
        assert!(term.grid().history_size() <= SCROLLBACK);
    }

    /// Output is hostile: whatever bytes come, reading and painting the screen goes on.
    #[test]
    fn hostile_output_breaks_nothing() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let pieces: [&[u8]; 12] = [
            b"\x1b[",
            b"\x1b]",
            b"8;;https://x\x1b\\",
            b"\x1b[?1049h",
            b"\x1b[999;999H",
            b"\x1b[38;2;300;1;1m",
            b"\x1b[48;5;999m",
            b"\x07",
            b"\xf0\x9f\x98",
            b"\xe2\x94\x80",
            b"\x1b[?2004h",
            b"\x1b[2J",
        ];
        for _ in 0..200 {
            let mut bytes = Vec::new();
            for _ in 0..(next() % 64) {
                match next() % 3 {
                    0 => bytes.extend_from_slice(pieces[(next() % pieces.len() as u64) as usize]),
                    1 => bytes.push((next() % 256) as u8),
                    _ => bytes.extend_from_slice(format!("{}", next() % 1000).as_bytes()),
                }
            }
            let term = fed(&bytes, 1 + (next() % 30) as usize, 1 + (next() % 10) as usize);
            let screen = snapshot(&term, &palette(), next() % 2 == 0);
            let _ = screen_text(&screen);
        }
    }
}
