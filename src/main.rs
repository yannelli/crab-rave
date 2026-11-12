mod art;
mod audio;
mod canvas;
mod crab;
mod scene;

use std::error::Error as _;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::{cursor, execute, terminal};

use crate::canvas::{Canvas, ColorMode};
use crate::scene::Rave;

const FRAME_TIME: Duration = Duration::from_millis(33);
const TITLE: &str = "🦀 CRAB RAVE 🦀";
const USAGE: &str = "Usage: crab-rave [--sound] [--duration SECONDS]";
const HELP: &str = "\
Usage: crab-rave [--sound] [--duration SECONDS]

Options:
  -s, --sound               Play the beat
  -d, --duration <SECONDS>  Stop after this many seconds
  -h, --help                Print help
  -V, --version             Print version

Press any key to stop.
";

static RESTORED: AtomicBool = AtomicBool::new(false);

enum Command {
    Run { duration: Option<f32>, sound: bool },
    Help,
    Version,
}

fn main() -> ExitCode {
    let (duration, sound) = match parse_args(std::env::args().skip(1)) {
        Ok(Command::Run { duration, sound }) => (duration, sound),
        Ok(Command::Help) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("crab-rave {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("crab-rave: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !io::stdout().is_terminal() {
        eprintln!("crab-rave: run this in a terminal");
        return ExitCode::FAILURE;
    }
    match run(duration, sound) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("crab-rave: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut duration = None;
    let mut sound = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "-s" | "--sound" => sound = true,
            "-d" | "--duration" => {
                let value = args.next().ok_or("--duration needs a number of seconds")?;
                let seconds = value
                    .parse::<f32>()
                    .ok()
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                    .ok_or_else(|| format!("invalid duration: {value}"))?;
                duration = Some(seconds);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Command::Run { duration, sound })
}

fn run(duration: Option<f32>, sound: bool) -> io::Result<()> {
    let iterm = std::env::var("TERM_PROGRAM").is_ok_and(|program| program == "iTerm.app");
    let color_mode = ColorMode::detect();
    let _audio = if sound {
        Some(audio::start().map_err(|error| {
            let cause = error
                .source()
                .map(|cause| format!(": {cause}"))
                .unwrap_or_default();
            io::Error::other(format!("cannot play sound: {error}{cause}"))
        })?)
    } else {
        None
    };
    let _session = Session::start(iterm)?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |since| since.as_nanos() as u64);
    let mut rave = Rave::new(seed);
    let mut stdout = io::stdout().lock();
    let mut frame = String::new();
    let start = Instant::now();

    loop {
        let frame_start = Instant::now();
        let time = start.elapsed().as_secs_f32();
        if duration.is_some_and(|limit| time >= limit) {
            return Ok(());
        }
        let (width, height) = terminal::size()?;
        let mut canvas = Canvas::new(width, height);
        let shake = rave.draw(&mut canvas, time);

        frame.clear();
        frame.push_str("\x1b[?2026h");
        canvas.encode(&mut frame, color_mode, shake);
        if iterm {
            rave.iterm_effects(&mut frame, time, width, height);
        }
        frame.push_str("\x1b[?2026l");
        stdout.write_all(frame.as_bytes())?;
        stdout.flush()?;

        if event::poll(FRAME_TIME.saturating_sub(frame_start.elapsed()))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            return Ok(());
        }
    }
}

struct Session {
    iterm: bool,
}

impl Session {
    fn start(iterm: bool) -> io::Result<Session> {
        terminal::enable_raw_mode()?;
        let session = Session { iterm };
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal(iterm);
            default_hook(info);
        }));
        let mut stdout = io::stdout();
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
        write!(stdout, "\x1b[22;0t\x1b]2;{TITLE}\x07")?;
        stdout.flush()?;
        Ok(session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        restore_terminal(self.iterm);
    }
}

fn restore_terminal(iterm: bool) {
    if RESTORED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut stdout = io::stdout();
    if iterm {
        let _ = write!(stdout, "\x1b]6;1;bg;*;default\x07");
    }
    let _ = write!(stdout, "\x1b[0m\x1b[23;0t");
    let _ = execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
}
