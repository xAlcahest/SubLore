//! Shared scaffolding for the ASR behavioural tests. See BACKLOG.md M3.1.
//!
//! Missing prerequisites are failures with an actionable message, never skips: the harness rule
//! the repo already follows in `e2e/lib/paths.js`.
//!
//! Each test binary compiles the whole module and uses part of it, so the unused half is not a
//! finding here.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use sublore_asr::tools::Tools;

/// How many entries a directory holds. Absent is zero, which is the pass these callers want; any
/// other read failure is a failure rather than a clean directory. See BACKLOG.md N9, S13.
pub fn entries_in(dir: &Path) -> usize {
    match fs::read_dir(dir) {
        Ok(entries) => entries.count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => panic!("{} should be readable or absent: {error}", dir.display()),
    }
}

/// A directory the test owns, removed when the test ends however it ends.
pub struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    pub fn new(name: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "sublore-asr-{}-{}-{name}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox directory should be creatable");
        Self { root }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn dir(&self) -> &Path {
        &self.root
    }

    /// A script for `fake_whisper`, one directive per line, handed over as the model path.
    pub fn script(&self, name: &str, lines: &[&str]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, lines.join("\n")).expect("the script should be writable");
        path
    }

    /// A real, decodable WAV of silence: ffmpeg has to be able to read it, so it cannot be a stub.
    pub fn silence(&self, name: &str, millis: u32) -> PathBuf {
        let samples = (16_000u64 * u64::from(millis) / 1000) as usize;
        let data = vec![0u8; samples * 2];
        let mut wav = Vec::with_capacity(data.len() + 44);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&32_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        let path = self.path(name);
        fs::write(&path, wav).expect("the fixture audio should be writable");
        path
    }

    /// Tools wired to the fake binary for both compute modes, so no whisper build is needed.
    pub fn tools(&self) -> Tools {
        self.tools_with(fake_whisper(), fake_whisper())
    }

    pub fn tools_with(&self, gpu: PathBuf, cpu: PathBuf) -> Tools {
        Tools {
            whisper_gpu: Some(gpu),
            whisper_cpu: cpu,
            ffmpeg: ffmpeg(),
            scratch_root: self.path("scratch"),
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn fake_whisper() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_whisper"))
}

pub fn ffmpeg() -> PathBuf {
    let name = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(&name))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| {
            panic!("ffmpeg is not in PATH; the ASR tests extract audio with it (see README.md)")
        })
}

/// As much of a process name as `comm` can hold: Linux truncates it to `TASK_COMM_LEN - 1`, so a
/// longer needle matches nothing and the negative checks below pass forever. See BACKLOG.md N9, S14.
pub fn comm_prefix(name: &str) -> &str {
    const COMM_MAX: usize = 15;
    match name.char_indices().nth(COMM_MAX) {
        Some((end, _)) => &name[..end],
        None => name,
    }
}

/// Whether a process id still has an entry in the process table, zombies included. A killed child
/// that was never waited for is a zombie, and a zombie is exactly what the M3.1 criterion forbids.
pub fn process_present(pid: u32, name: &str) -> bool {
    #[cfg(unix)]
    {
        let output = Command::new("ps")
            .args(["-o", "stat=,comm=", "-p", &pid.to_string()])
            .output()
            .expect("ps should be runnable on a unix machine");
        let text = String::from_utf8_lossy(&output.stdout);
        // The name check makes a recycled pid read as gone rather than as a survivor.
        text.contains(comm_prefix(name))
    }
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist should be runnable on Windows");
        String::from_utf8_lossy(&output.stdout).contains(name)
    }
}

/// Everything about a file that must not change when a run reads it.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub len: u64,
    pub modified: Option<std::time::SystemTime>,
    pub siblings: Vec<String>,
}

pub fn snapshot(path: &Path) -> Snapshot {
    let metadata = fs::metadata(path).expect("the file under test should exist");
    let mut siblings: Vec<String> = fs::read_dir(path.parent().expect("a parent directory"))
        .expect("the directory should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    siblings.sort();
    Snapshot {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        siblings,
    }
}

// ---------------------------------------------------------------------------
// A local HTTP server, so the download is exercised over a real socket with a real client and
// still never touches the internet. See BACKLOG.md M3.2.
// ---------------------------------------------------------------------------

use std::io::{BufRead, BufReader, Read as _, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;

/// The 300-byte stand-in for a model. Deterministic, so its sha256 can be a constant.
pub fn test_model_body() -> Vec<u8> {
    (0..300u32).map(|index| (index * 7 % 256) as u8).collect()
}

pub const TEST_MODEL_SHA256: &str =
    "9a76b8af8f16f19d60de2b3999c22f9d10be4395c90ea3bfc5eb6cd6254243af";

#[derive(Clone, Debug, Default)]
pub struct Policy {
    /// Answer `Range` with a 206. Off means the server sends the whole file every time, which is
    /// what a proxy or a mirror that does not support ranges does.
    pub honour_range: bool,
    /// Send this many body bytes and hang up, mid-transfer.
    pub cut_after: Option<usize>,
    /// Send the body without a `Content-Length`, and this many extra bytes after it.
    pub overrun: Option<usize>,
    /// Declare a `Content-Length` this far from the truth.
    pub lie_about_length: Option<u64>,
}

pub struct FakeServer {
    port: u16,
    accepts: Arc<AtomicUsize>,
    /// The offset of the last `Range` header seen, so a test can prove a resume asked for one.
    range: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl FakeServer {
    pub fn start(body: Vec<u8>, policy: Policy) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port should be free");
        let port = listener.local_addr().expect("a bound address").port();
        listener
            .set_nonblocking(true)
            .expect("the listener should accept a nonblocking mode");
        let accepts = Arc::new(AtomicUsize::new(0));
        let range = Arc::new(AtomicU64::new(u64::MAX));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let accepts = Arc::clone(&accepts);
            let range = Arc::clone(&range);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            accepts.fetch_add(1, Ordering::SeqCst);
                            serve(stream, &body, &policy, &range);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => return,
                    }
                }
            })
        };
        Self {
            port,
            accepts,
            range,
            stop,
            worker: Some(worker),
        }
    }

    /// A base URL shaped like the catalog's, so the code under test builds its URL the same way.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    pub fn accepts(&self) -> usize {
        self.accepts.load(Ordering::SeqCst)
    }

    /// The last `Range` offset asked for, or nothing if no request carried one.
    pub fn last_range(&self) -> Option<u64> {
        match self.range.load(Ordering::SeqCst) {
            u64::MAX => None,
            offset => Some(offset),
        }
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(mut stream: TcpStream, body: &[u8], policy: &Policy, range: &AtomicU64) {
    // The listener is non-blocking so the worker can notice `stop`. On Windows the accepted socket
    // inherits that flag and on Linux it does not, so `read_line` below could answer `WouldBlock`,
    // be read as EOF, and close the connection without a response. See BACKLOG.md M3.2.
    stream
        .set_nonblocking(false)
        .expect("the accepted socket should go back to blocking");
    let mut reader = BufReader::new(stream.try_clone().expect("the socket should clone"));
    let mut from = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        // Header names are case-insensitive, and a real client sends them lowercased.
        let lowered = trimmed.to_ascii_lowercase();
        if let Some(value) = lowered.strip_prefix("range: bytes=") {
            if let Some((start, _)) = value.split_once('-') {
                from = start.trim().parse().unwrap_or(0);
                range.store(from as u64, Ordering::SeqCst);
            }
        }
    }

    let ranged = policy.honour_range && from > 0 && from < body.len();
    let start = if ranged { from } else { 0 };
    let slice = &body[start..];
    let mut head = String::new();
    if ranged {
        head.push_str("HTTP/1.1 206 Partial Content\r\n");
        head.push_str(&format!(
            "Content-Range: bytes {start}-{}/{}\r\n",
            body.len() - 1,
            body.len()
        ));
    } else {
        head.push_str("HTTP/1.1 200 OK\r\n");
    }
    match (policy.overrun, policy.lie_about_length) {
        // No Content-Length at all: the body ends when the connection does, which is how a stream
        // can run past what the catalog says.
        (Some(_), _) => head.push_str("Connection: close\r\n"),
        (None, Some(length)) => head.push_str(&format!("Content-Length: {length}\r\n")),
        (None, None) => head.push_str(&format!("Content-Length: {}\r\n", slice.len())),
    }
    head.push_str("Accept-Ranges: bytes\r\n\r\n");
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }

    let sent = match policy.cut_after {
        Some(cut) => &slice[..cut.min(slice.len())],
        None => slice,
    };
    let _ = stream.write_all(sent);
    if let Some(extra) = policy.overrun {
        let _ = stream.write_all(&vec![0xAAu8; extra]);
    }
    let _ = stream.flush();
    // Send the FIN, then drain whatever the client still had to say, so the close is not a reset it
    // reports instead of the truncation under test. Bounded by a timeout: with keep-alive the
    // client has no reason to close first, so this waits out the bound on every connection.
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(200)));
    let mut sink = [0u8; 1024];
    while matches!(stream.read(&mut sink), Ok(read) if read > 0) {}
}
