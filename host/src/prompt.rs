//! The line the user types on. It stays at the bottom of the screen while
//! agents work, so the next request can be written, edited and sent before
//! the current one has finished: it waits in a queue. Up and Down go through
//! the requests of this session.
//!
//! The terminal is switched to deliver each key as it is pressed, without
//! echo; output and Ctrl-C keep working as usual.

use std::sync::mpsc::{channel, Receiver};
use std::thread;

use crate::activity::Activity;

/// The terminal's settings before the editor changed them.
static mut ORIGINAL: Option<libc::termios> = None;

fn restore() {
    // Only read here and written once in `start`, before the reader thread.
    #[allow(static_mut_refs)]
    if let Some(original) = unsafe { ORIGINAL.as_ref() } {
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original) };
    }
}

extern "C" fn interrupted(_: libc::c_int) {
    restore();
    unsafe {
        libc::write(libc::STDERR_FILENO, b"\n".as_ptr().cast(), 1);
        libc::_exit(130);
    }
}

fn byte() -> Option<u8> {
    let mut one = 0u8;
    let n = unsafe { libc::read(libc::STDIN_FILENO, (&mut one as *mut u8).cast(), 1) };
    (n == 1).then_some(one)
}

enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    KillLine,
    KillToEnd,
    KillWord,
    Eof,
    Other,
}

/// Reads one key: a character, a control key, or an escape sequence.
fn key() -> Option<Key> {
    Some(match byte()? {
        b'\r' | b'\n' => Key::Enter,
        0x7f | 0x08 => Key::Backspace,
        0x01 => Key::Home,
        0x05 => Key::End,
        0x02 => Key::Left,
        0x06 => Key::Right,
        0x10 => Key::Up,
        0x0e => Key::Down,
        0x15 => Key::KillLine,
        0x0b => Key::KillToEnd,
        0x17 => Key::KillWord,
        0x04 => Key::Eof,
        0x1b => match byte()? {
            // ESC [ <numbers> <letter>, or ESC O <letter> from some terminals.
            b'[' | b'O' => {
                let mut sequence = Vec::new();
                loop {
                    let b = byte()?;
                    sequence.push(b);
                    if (0x40..=0x7e).contains(&b) {
                        break;
                    }
                }
                match sequence.as_slice() {
                    b"A" => Key::Up,
                    b"B" => Key::Down,
                    b"C" => Key::Right,
                    b"D" => Key::Left,
                    b"H" | b"1~" | b"7~" => Key::Home,
                    b"F" | b"4~" | b"8~" => Key::End,
                    b"3~" => Key::Delete,
                    _ => Key::Other,
                }
            }
            _ => Key::Other,
        },
        first if first < 0x20 => Key::Other,
        first => {
            // A character of one to four bytes.
            let length = match first {
                0xf0.. => 4,
                0xe0.. => 3,
                0xc0.. => 2,
                _ => 1,
            };
            let mut bytes = vec![first];
            for _ in 1..length {
                bytes.push(byte()?);
            }
            std::str::from_utf8(&bytes).ok().and_then(|s| s.chars().next()).map_or(Key::Other, Key::Char)
        }
    })
}

/// What is being typed, and what was sent before.
#[derive(Default)]
struct Editor {
    text: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    /// Which past request is shown; `history.len()` is the new line.
    at: usize,
    /// The new line, kept while looking through the history.
    draft: Vec<char>,
}

impl Editor {
    fn show(&mut self, at: usize) {
        if self.at == self.history.len() {
            self.draft = self.text.clone();
        }
        self.at = at;
        self.text = self.history.get(at).map_or_else(|| self.draft.clone(), |line| line.chars().collect());
        self.cursor = self.text.len();
    }

    /// Applies a key. Returns the line when it was sent.
    fn press(&mut self, key: Key) -> Option<String> {
        match key {
            Key::Char(c) => {
                self.text.insert(self.cursor, c);
                self.cursor += 1;
            }
            Key::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.text.remove(self.cursor);
            }
            Key::Delete if self.cursor < self.text.len() => {
                self.text.remove(self.cursor);
            }
            Key::Left => self.cursor = self.cursor.saturating_sub(1),
            Key::Right => self.cursor = (self.cursor + 1).min(self.text.len()),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.text.len(),
            Key::Up if self.at > 0 => self.show(self.at - 1),
            Key::Down if self.at < self.history.len() => self.show(self.at + 1),
            Key::KillLine => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            Key::KillToEnd => self.text.truncate(self.cursor),
            Key::KillWord => {
                let before = &self.text[..self.cursor];
                let word_end = before.iter().rposition(|c| !c.is_whitespace()).map_or(0, |i| i + 1);
                let start = before[..word_end].iter().rposition(|c| c.is_whitespace()).map_or(0, |i| i + 1);
                self.text.drain(start..self.cursor);
                self.cursor = start;
            }
            Key::Enter => {
                let line: String = self.text.drain(..).collect::<String>().trim().to_string();
                self.cursor = 0;
                self.draft.clear();
                if line.is_empty() {
                    return None;
                }
                if self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                }
                self.at = self.history.len();
                return Some(line);
            }
            _ => {}
        }
        None
    }
}

/// The requests the user sends, in order. Reading them never waits for the
/// user to stop typing the next one.
pub struct Requests {
    sent: Receiver<String>,
    activity: Activity,
}

impl Requests {
    /// Starts the editor when both ends are a terminal. Otherwise there is
    /// none, and the caller reads plain lines.
    pub fn start(activity: &Activity) -> Option<Self> {
        if !activity.is_live() || unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
            return None;
        }
        unsafe {
            let mut settings: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut settings) != 0 {
                return None;
            }
            ORIGINAL = Some(settings);
            // Keys arrive one by one and are not echoed. Signals and output stay as they were.
            settings.c_lflag &= !(libc::ICANON | libc::ECHO);
            settings.c_cc[libc::VMIN] = 1;
            settings.c_cc[libc::VTIME] = 0;
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &settings);
            let handler = interrupted as extern "C" fn(libc::c_int) as libc::sighandler_t;
            libc::signal(libc::SIGINT, handler);
            libc::signal(libc::SIGTERM, handler);
        }

        let (send, sent) = channel();
        let screen = activity.clone();
        screen.prompt(|_| ());
        thread::spawn(move || {
            let mut editor = Editor::default();
            while let Some(key) = key() {
                let eof = matches!(key, Key::Eof) && editor.text.is_empty();
                let line = if eof { Some("/exit".to_string()) } else { editor.press(key) };
                screen.prompt(|prompt| {
                    prompt.text = editor.text.clone();
                    prompt.cursor = editor.cursor;
                    prompt.queued.extend(line.clone());
                });
                if let Some(line) = line {
                    if send.send(line).is_err() {
                        break;
                    }
                }
            }
        });
        Some(Self { sent, activity: activity.clone() })
    }

    /// The next request, waiting for one if the queue is empty. It leaves
    /// the queue and is written on screen as the request now being worked on.
    pub fn next(&self) -> Option<String> {
        let line = self.sent.recv().ok()?;
        self.activity.prompt(|prompt| {
            if !prompt.queued.is_empty() {
                prompt.queued.remove(0);
            }
        });
        self.activity.say(&format!("> {line}"));
        Some(line)
    }
}

impl Drop for Requests {
    fn drop(&mut self) {
        self.activity.close_prompt();
        restore();
    }
}
