//! Interactive probe built against the pinned, patched crossterm in Windows CI.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

type Handle = *mut c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(which: u32) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
}

struct RawInput(Handle, u32);

impl RawInput {
    fn new() -> io::Result<Self> {
        // GetStdHandle returns a borrowed handle; it must not be closed here.
        let input = unsafe { GetStdHandle((-10i32) as u32) };
        let mut original_mode = 0;
        if unsafe { GetConsoleMode(input, &mut original_mode) } == 0 {
            return Err(io::Error::last_os_error());
        }
        enable_raw_mode()?;
        let restore = Self(input, original_mode);
        let mut mode = 0;
        if unsafe { GetConsoleMode(input, &mut mode) } == 0
            || unsafe { SetConsoleMode(input, mode & !0x0200) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // Like Codex, use Win32 input records rather than inherited VT input.
        Ok(restore)
    }
}

impl Drop for RawInput {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        unsafe { SetConsoleMode(self.0, self.1) };
    }
}

fn run() -> io::Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs();
    let path = std::env::current_exe()?
        .with_file_name(format!("crossterm-keys-{stamp}-{}.log", std::process::id()));
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let _restore = RawInput::new()?;
    let variant = option_env!("CTRL_C_PROBE_VARIANT").unwrap_or("unlabelled");
    println!("Actual crossterm input probe: {variant}.\r");
    writeln!(log, "variant={variant}")?;
    println!("Press Ctrl+C, Ctrl+D and Ctrl+Shift+C in each relevant layout. F12 exits.\r");
    println!("Each event should contain an ASCII letter and CONTROL in every layout.\r");
    println!("Log: {}\r", path.display());

    loop {
        let event = event::read()?;
        let ctrl_letter = matches!(
            event,
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char(c) if c.is_ascii_alphabetic())
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
        );
        println!("{event:?} ctrl_letter={ctrl_letter}\r");
        writeln!(log, "{event:?} ctrl_letter={ctrl_letter}")?;
        log.flush()?;
        if matches!(event, Event::Key(key) if key.kind == KeyEventKind::Press && key.code == KeyCode::F(12))
        {
            break;
        }
    }
    println!("Saved {}\r", path.display());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Probe failed: {error}");
        std::process::exit(1);
    }
}
