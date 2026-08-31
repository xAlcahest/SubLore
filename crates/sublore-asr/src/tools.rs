//! Finding the programs a run spawns, once, before anything is started. See BACKLOG.md M3.1.
//!
//! Nothing here downloads, installs or repairs anything: a missing program is a sentence the user
//! can act on, never a silent no-op and never a fetch they did not ask for (CONTRIBUTING.md §1).

use std::env;
use std::path::{Path, PathBuf};

use crate::error::{AsrError, AsrErrorKind};
use crate::sidecar::Compute;

/// Absolute path to a whisper binary, overriding discovery. Used by the E2E harness and by
/// developers who built their own; when it is set both compute modes run it, and the CPU mode
/// still passes `-ng`, which produces the same output a CPU-only build would.
pub const WHISPER_BIN_ENV: &str = "SUBLORE_WHISPER_BIN";
/// Absolute path to ffmpeg, overriding discovery.
pub const FFMPEG_BIN_ENV: &str = "SUBLORE_FFMPEG_BIN";

/// The Vulkan build. Optional: without it, a GPU request runs on the CPU and says so.
const WHISPER_GPU: &str = "whisper-cli";
/// The CPU-only build. Required, and the reason the fallback is a property of what we ship rather
/// than of the user's driver stack (CONTRIBUTING.md §2).
const WHISPER_CPU: &str = "whisper-cli-cpu";
const FFMPEG: &str = "ffmpeg";
/// Where `scripts/build-whisper.sh` leaves its output, relative to the repo root.
const DEV_BIN_DIR: [&str; 2] = [".whisper", "bin"];
/// How far up from the running executable the repo root is looked for.
const DEV_WALK_UP: usize = 6;

/// Where the run's programs and working space are. Held by the app for the whole session.
#[derive(Clone, Debug)]
pub struct Tools {
    /// The Vulkan binary, when there is one.
    pub whisper_gpu: Option<PathBuf>,
    pub whisper_cpu: PathBuf,
    pub ffmpeg: PathBuf,
    /// `app_data_dir()/scratch`. Every intermediate file lives under it and nowhere else: never
    /// beside the user's media, never in the system temp dir (CONTRIBUTING.md §3).
    pub scratch_root: PathBuf,
}

impl Tools {
    /// Look for everything a run needs. `resource_dir` is where the installer puts the binaries;
    /// pass `None` outside a packaged app.
    pub fn discover(resource_dir: Option<&Path>, scratch_root: PathBuf) -> Result<Self, AsrError> {
        let override_path = env_path(WHISPER_BIN_ENV);
        let (whisper_gpu, whisper_cpu) = match override_path {
            Some(path) => {
                if !path.is_file() {
                    return Err(AsrError::new(
                        AsrErrorKind::BinaryMissing,
                        format!(
                            "{WHISPER_BIN_ENV} points at {}, which is not a file",
                            path.display()
                        ),
                    ));
                }
                (Some(path.clone()), path)
            }
            None => {
                let roots = search_roots(resource_dir);
                let cpu = find_in(&roots, WHISPER_CPU).or_else(|| find_in(&roots, WHISPER_GPU));
                let Some(cpu) = cpu else {
                    return Err(AsrError::new(
                        AsrErrorKind::BinaryMissing,
                        format!(
                            "no {WHISPER_CPU} in {} or PATH; run scripts/build-whisper.sh",
                            display_roots(&roots)
                        ),
                    ));
                };
                (find_in(&roots, WHISPER_GPU), cpu)
            }
        };

        let ffmpeg = match env_path(FFMPEG_BIN_ENV) {
            Some(path) if path.is_file() => path,
            Some(path) => {
                return Err(AsrError::new(
                    AsrErrorKind::FfmpegMissing,
                    format!(
                        "{FFMPEG_BIN_ENV} points at {}, which is not a file",
                        path.display()
                    ),
                ))
            }
            None => find_on_path(FFMPEG).ok_or_else(|| {
                AsrError::new(
                    AsrErrorKind::FfmpegMissing,
                    "no ffmpeg in PATH; Sublore extracts audio with it".to_owned(),
                )
            })?,
        };

        Ok(Self {
            whisper_gpu,
            whisper_cpu,
            ffmpeg,
            scratch_root,
        })
    }

    /// The binary that runs for `compute`, and whether asking for the GPU landed on the CPU.
    pub fn whisper(&self, compute: Compute) -> (&Path, bool) {
        match compute {
            Compute::Cpu => (&self.whisper_cpu, false),
            Compute::Gpu => match &self.whisper_gpu {
                Some(path) => (path, false),
                None => (&self.whisper_cpu, true),
            },
        }
    }
}

/// A set variable that is empty is not a path; treat it as unset rather than as `.`.
fn env_path(name: &str) -> Option<PathBuf> {
    let value = env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// Directories searched before PATH, nearest first: what the installer laid down, then what
/// `scripts/build-whisper.sh` built, so `pnpm tauri dev` and the E2E harness work uninstalled.
fn search_roots(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = resource_dir {
        roots.push(dir.to_path_buf());
    }
    if let Ok(exe) = env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..DEV_WALK_UP {
            let Some(current) = dir else { break };
            let candidate = DEV_BIN_DIR
                .iter()
                .fold(current.clone(), |acc, part| acc.join(part));
            if candidate.is_dir() {
                roots.push(candidate);
                break;
            }
            dir = current.parent().map(Path::to_path_buf);
        }
    }
    roots
}

fn find_in(roots: &[PathBuf], name: &str) -> Option<PathBuf> {
    for root in roots {
        let candidate = root.join(with_exe_suffix(name));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    find_on_path(name)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let name = with_exe_suffix(name);
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|dir| dir.join(&name))
            .find(|candidate| candidate.is_file())
    })
}

fn with_exe_suffix(name: &str) -> String {
    format!("{name}{}", env::consts::EXE_SUFFIX)
}

fn display_roots(roots: &[PathBuf]) -> String {
    if roots.is_empty() {
        return "no install or build directory".to_owned();
    }
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{find_in, with_exe_suffix, Tools};
    use crate::sidecar::Compute;
    use std::path::PathBuf;

    fn tools(gpu: Option<&str>) -> Tools {
        Tools {
            whisper_gpu: gpu.map(PathBuf::from),
            whisper_cpu: PathBuf::from("/cpu"),
            ffmpeg: PathBuf::from("/ffmpeg"),
            scratch_root: PathBuf::from("/scratch"),
        }
    }

    #[test]
    fn a_gpu_request_runs_the_cpu_binary_when_no_vulkan_build_is_installed() {
        let tools = tools(None);
        let (path, fell_back) = tools.whisper(Compute::Gpu);
        assert_eq!(path, PathBuf::from("/cpu"));
        assert!(fell_back, "the caller must be able to say so in the UI");
    }

    #[test]
    fn each_compute_mode_picks_its_own_binary_when_both_exist() {
        let tools = tools(Some("/gpu"));
        assert_eq!(
            tools.whisper(Compute::Gpu),
            (PathBuf::from("/gpu").as_path(), false)
        );
        assert_eq!(
            tools.whisper(Compute::Cpu),
            (PathBuf::from("/cpu").as_path(), false)
        );
    }

    #[test]
    fn a_directory_that_holds_nothing_is_not_a_hit() {
        let roots = vec![PathBuf::from("/nonexistent-sublore-root")];
        assert_eq!(find_in(&roots, "sublore-no-such-program-4718"), None);
    }

    #[test]
    fn windows_binaries_carry_their_suffix() {
        let expected = format!("whisper-cli{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(with_exe_suffix("whisper-cli"), expected);
    }
}
