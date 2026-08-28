//! A stand-in for whisper-cli that does exactly what a test tells it to. See BACKLOG.md M3.1.
//!
//! It is what makes the sidecar's behaviour — progress, cancellation, reaping, the stall timer,
//! the exit codes that lie — testable on a machine with no whisper build and no model, which is
//! what CI is on every pull request.
//!
//! The script is the file passed as `-m`, one directive per line, run in order. Putting it there
//! rather than in an environment variable keeps concurrent tests from overwriting each other's
//! instructions.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Exit code for "the test set this up wrong", distinct from anything whisper itself returns.
const EXIT_MISUSE: i32 = 9;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let script_path = match flag(&argv, "-m") {
        Some(path) => PathBuf::from(path),
        None => misuse("fake_whisper needs -m <script>"),
    };
    let stem = flag(&argv, "-of").map(PathBuf::from);
    let script = match fs::read_to_string(&script_path) {
        Ok(script) => script,
        Err(error) => misuse(&format!("cannot read {}: {error}", script_path.display())),
    };

    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (directive, rest) = line.split_once(' ').unwrap_or((line, ""));
        match directive {
            "progress" => {
                let percent = number(rest, directive);
                say_err(&format!(
                    "whisper_print_progress_callback: progress = {percent:3}%"
                ));
            }
            "segment" => {
                let (ms, text) = rest.split_once(' ').unwrap_or((rest, ""));
                say_out(&format!(
                    "[00:00:00.000 --> {}]   {text}",
                    stamp(number(ms, directive))
                ));
            }
            "noise" => say_err(rest),
            "sleep" => {
                std::thread::sleep(std::time::Duration::from_millis(number(rest, directive)))
            }
            "sleep-forever" => loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            },
            "stderr-flood" => {
                let total = number(rest, directive) as usize;
                let chunk = "x".repeat(1023);
                let mut written = 0;
                while written < total {
                    say_err(&chunk);
                    written += chunk.len() + 1;
                }
            }
            "unknown-argument" => say_err("error: unknown argument: -zz"),
            "pid" => write_file(&PathBuf::from(rest), &std::process::id().to_string()),
            "argv" => write_file(&PathBuf::from(rest), &argv.join("\n")),
            "json" => {
                let Some(stem) = stem.as_ref() else {
                    misuse("the script writes JSON but no -of was given")
                };
                let body = match fs::read(rest) {
                    Ok(body) => body,
                    Err(error) => misuse(&format!("cannot read {rest}: {error}")),
                };
                let target = stem.with_extension("json");
                if let Err(error) = fs::write(&target, body) {
                    misuse(&format!("cannot write {}: {error}", target.display()));
                }
            }
            "exit" => std::process::exit(number(rest, directive) as i32),
            other => misuse(&format!("unknown directive {other:?}")),
        }
    }
}

/// The value after `name` in argv, the way whisper takes its options.
fn flag<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
    let index = argv.iter().position(|argument| argument == name)?;
    argv.get(index + 1).map(String::as_str)
}

fn number(text: &str, directive: &str) -> u64 {
    match text.trim().parse() {
        Ok(value) => value,
        Err(_) => misuse(&format!("{directive} needs a number, got {text:?}")),
    }
}

fn stamp(ms: u64) -> String {
    let (hours, minutes, seconds, millis) = (
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000,
    );
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn say_out(line: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn say_err(line: &str) {
    let mut err = io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

fn write_file(path: &PathBuf, body: &str) {
    if let Err(error) = fs::write(path, body) {
        misuse(&format!("cannot write {}: {error}", path.display()));
    }
}

/// Loud on purpose: a mistake in a test script must not look like a whisper failure.
fn misuse(message: &str) -> ! {
    say_err(&format!("fake_whisper: {message}"));
    std::process::exit(EXIT_MISUSE)
}
