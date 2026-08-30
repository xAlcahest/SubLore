# Gate 2 — L8: `startup_files` as an input surface, and its frontend consumer

GATE_BASE=f0b0058, GATE_HEAD=eca9806. Scope: `src-tauri/src/lib.rs` (`startup_files`,
`startup_files_command`), `src/hooks/useStartupFiles.ts`, `src/App.tsx`. All three were introduced
or touched by `062f201` and are in range.

## What I checked

- Read `startup_files` and `startup_files_command` whole (`src-tauri/src/lib.rs:24-64`), confirmed
  against `git diff f0b0058 eca9806 -- src-tauri/src/lib.rs` that this is new code from `062f201`,
  unmodified since.
- Read `src/hooks/useStartupFiles.ts` whole (49 lines, new file) and its call site in
  `src/App.tsx:23` (`useStartupFiles(open, subtitle.open)`).
- Traced both callbacks passed in: `open` from `src/hooks/useVideoPlayer.ts:73-87` and
  `subtitle.open` (`= serialize(() => openFile(path))`) from `src/hooks/useSubtitleFile.ts:130-174`,
  including the `serialize` queue (`useSubtitleFile.ts:116-121`) and the error-normalizing helpers
  `toErrorCode` (`useVideoPlayer.ts:31-33`) and `toSubtitleError` (`useSubtitleFile.ts:66-70`), to
  determine whether either callback can actually reject.
- Verified `is_file()` behaviour on a FIFO and `/dev/null` empirically (compiled and ran a small
  Rust program under `rustc`, not just read the stdlib docs), and verified that a real file named
  with a leading dash exists on disk but is excluded by the argument filter before the existence
  check ever runs.
- Confirmed `.manage(startup_files(...))` (`lib.rs:75`) runs before `.invoke_handler(...)` (`lib.rs:76`)
  in the same builder chain, and that `startup_files_command` (`lib.rs:61-64`) only clones already-
  computed state — no I/O, no mutation.
- Checked for a `.desktop` file or packaged launcher script anywhere in the tree (none exists yet)
  and for `tauri-plugin-single-instance` or file-association config in `tauri.conf.json` (neither is
  present), to evaluate the `.skip(1)` / argv[0] assumption.
- Grepped `useStartupFiles.ts` and the `startup_files` path in `lib.rs` for hardcoded user-facing
  strings (CLAUDE.md §9's i18n rule).

## Findings

### 1. (minor) Argument names beginning with `-` are dropped unconditionally, even when they name a real file, with no feedback anywhere

`src-tauri/src/lib.rs:45`:

```rust
.filter(|a| !a.starts_with('-') && std::path::Path::new(a).is_file())
```

`&&` short-circuits on `starts_with('-')`, so the existence check never even runs for such an
argument. Verified empirically: created `/tmp/-dashfile.srt` (a real, existing regular file) and
confirmed `Path::new("/tmp/-dashfile.srt").is_file()` returns `true` — the file would pass the
existence check if it were reached, but the dash filter removes it first regardless.

**Failure:** `sublore -export.srt movie.mkv`, where `-export.srt` is a real subtitle file (a
plausible name from any export tool or numbering scheme that produces leading-dash filenames, or
simply a file the user renamed). The subtitle argument is silently discarded before the `is_file()`
check runs. The app starts with the video loaded and no subtitle open, and nothing — no dialog, no
log line — tells the user their second argument was ignored. This is exactly the "surprising input
at a boundary" this lens is chartered to find; per the lens's own framing it is a robustness/
error-reporting defect, not a security one, since the user is the one who typed the command line.

### 2. (minor) No argv element that fails the filter is ever logged, anywhere in this path

`src-tauri/src/lib.rs:43-46`. Beyond the dash case above, any argument that fails `is_file()` —
a typo'd path, a path that existed when the shell command was built but was deleted before launch,
a FIFO, a device node, a directory — is dropped the same way: no `log::warn!`, no `log::info!`,
nothing written anywhere, not even to the rotating log file that every other startup step
(`lib.rs:110-114`) writes to.

**Failure:** `sublore epsiode.srt` (typo — file does not exist). `Path::new("epsiode.srt").is_file()`
returns `false`, the argument is dropped, `startup_files` returns `StartupFiles { video: None,
subtitle: None }`, and the app opens with nothing loaded. Nothing in the UI or in the log records
that "epsiode.srt" was ever named on the command line. CLAUDE.md §6 asks that errors "surface to the
UI as actionable messages, never silent logs" — this case is stricter than a silent log: it is
silent everywhere. Grouped with finding 1 because both share the same root cause (no feedback path
exists for any excluded argv element) and the same fix shape (log what was dropped and why).

### 3. (minor) `useStartupFiles.ts`'s two `await` calls outside the `try/catch` are a real contract gap, but do not currently produce the failure the brief describes — correcting that claim

`src/hooks/useStartupFiles.ts:30-47`. The `try/catch` at lines 32-38 covers only
`invoke("startup_files_command")`. `await openVideo(files.video)` (line 42) and
`await openSubtitle(files.subtitle)` (line 45) sit outside it, inside a `void (async () => {...})()`
IIFE, so a rejection from either would be an unhandled promise rejection with no message shown to
the user.

I traced both callbacks as actually wired in `App.tsx:23`, `useStartupFiles(open, subtitle.open)`:

- `open` (`useVideoPlayer.ts:73-87`) wraps its whole body in `try { ... } catch (error) {
setErrorCode(toErrorCode(error)); }` and never rethrows. `toErrorCode` (`:31-33`) is a pure,
  non-throwing function.
- `subtitle.open` = `serialize(() => openFile(path))` (`useSubtitleFile.ts:171-174`). `openFile`
  (`:137-169`) likewise wraps its body in `try/catch` and never rethrows; `toSubtitleError`
  (`:66-70`) is pure and non-throwing. `serialize` (`:116-121`) chains `work` onto a queue and
  explicitly comments "every unit below catches its own failure, so the chain cannot reject."

So, as the code stands today, neither `openVideo` nor `subtitle.open` can reject, which means:
**the hunt list's specific scenario — "a rejection there is an unhandled promise rejection" and "a
failing video open means the subtitle is never opened at all" — does not currently occur.** Both
calls always resolve, both errors are already surfaced through existing UI state
(`errorCode` → `App.tsx:52-55`; `subtitle.error` → passed to `SubtitleBar` at `App.tsx:61`), and a
failing video open does not block the subsequent subtitle open, since `openVideo` resolves normally
even when the underlying `video_open` invoke rejects.

That said, the underlying code-quality defect is real and I report it as the corrected version: the
hook's own declared type, `(path: string) => Promise<unknown>`, does not promise the callback can
never reject, and the hook's implementation has no defensive catch around either call. It is a
boundary consumer with no boundary — it currently survives only because both of today's callers
independently chose to swallow their own errors, a fact `useStartupFiles.ts` never asserts or relies
on explicitly. A future, entirely reasonable change to either hook (e.g. rethrowing after setting
error state, which is a natural refactor of exactly this kind of code) would silently reintroduce
the failure mode the brief describes, with zero test or type system signal that it had done so.
Rated minor because it is latent, not live.

### 4. (minor) The broad `catch` around `invoke("startup_files_command")` swallows more than its comment claims

`src/hooks/useStartupFiles.ts:32-38`:

```ts
try {
  files = await invoke<StartupFiles>("startup_files_command");
} catch {
  // Nothing was asked for, or the command is unavailable: the app is perfectly usable
  // without it, so this stays silent rather than showing an error nobody caused.
  return;
}
```

The comment's first clause is wrong about what triggers this branch: "nothing was asked for" (no
files on argv) is _not_ an error case — `invoke` resolves normally with
`{ video: null, subtitle: null }` and the `catch` is never entered for it. The `catch` only fires on
an actual `invoke` rejection, i.e. a real backend failure. In practice `startup_files_command`
(`lib.rs:61-64`) is a pure clone of state that is `.manage`d (`lib.rs:75`) before `.invoke_handler`
registers the command (`lib.rs:76`) and before `.setup()` runs, so it has no realistic failure mode
today, which keeps this low-severity. But the comment documents a justification that does not match
the code's actual branching, and if `startup_files_command` ever grew a fallible step, this catch
would discard that failure with no message and no log line, the same silent-everywhere pattern as
finding 2.

## Hunt items checked and found sound

- **FIFOs, device nodes, directories on argv.** Empirically confirmed (`rustc`-compiled test
  program): `Path::new(fifo).is_file()` and `Path::new("/dev/null").is_file()` both return `false`.
  Neither is classified as video or subtitle; both are dropped by the existing `is_file()` filter
  exactly as intended. No misclassification risk from this class of input.
- **Extension classification (`lib.rs:47-56`).** `to_lowercase()` + `ends_with` is case-insensitive
  and suffix-anchored, so `.SRT`, `movie.srt.bak` (correctly _not_ matched), and any non-subtitle
  extension are classified as documented. A wrong guess (e.g. a random non-media file becoming
  `files.video`) is reported through the existing, already-wired video error UI
  (`App.tsx:52-55`, `videoErrorMessage`) when the frontend tries to open it — this satisfies the
  brief's own "does not count" carve-out ("the question is whether a wrong guess is reported").
- **Two videos / two subtitles / ten files named at once.** `get_or_insert` keeps only the first
  match per category; later ones of the same kind are dropped with no indication. I considered this
  as a candidate finding but did not raise it separately: `StartupFiles` has exactly one slot per
  kind by design, and there is no sensible alternative behaviour for a two-slot struct fed more than
  two files. It shares the same "no feedback" root cause as findings 1 and 2 and is covered by their
  fix, not raised as its own item.
- **TOCTOU between the `is_file()` check at startup and the actual open "seconds later" in the
  webview.** Traced the full path: even if a file is deleted or replaced between the two points, the
  eventual `invoke("video_open" | "subtitle_open")` call performs its own I/O and its own error
  path, and that error path is already wired to user-visible state (`errorCode`, `subtitle.error`)
  independent of anything `startup_files` did. The early check is a soft filter, not a security or
  correctness gate; a race here degrades to an already-handled "open failed" error, not a silent or
  unreported failure.
- **`startup_files_command` registered before `.setup()`, state presence for the first invoke, and
  determinism across repeated calls.** `.manage(startup_files(...))` (`lib.rs:75`) executes in the
  same builder chain before `.invoke_handler` (`lib.rs:76`) and before `.setup()` (`lib.rs:107`)
  runs; Tauri's managed state is available to any command from the first invoke onward. The command
  itself (`lib.rs:61-64`) is `state.inner().clone()` — pure, no mutation, so calling it any number of
  times returns the same value. Confirmed sound.
- **The `started` ref (`useStartupFiles.ts:22-28`) versus the `[openVideo, openSubtitle]` dependency
  array.** The ref, not the dependency array, is what actually prevents a re-run: it is checked and
  set unconditionally at the top of the effect body before anything else happens, so even a changing
  dependency identity cannot cause a second invocation once `started.current` is `true`. In the
  current wiring both `open` (`useVideoPlayer.ts:73`, `useCallback` with `[]`) and `subtitle.open`
  (`useSubtitleFile.ts:171-174`, `useCallback` over stable `openFile`/`serialize`) are themselves
  referentially stable across renders, so the dependency array would not have re-triggered the
  effect even without the guard — but the guard is the actual mechanism, matching what the brief
  asked to confirm.
- **`.skip(1)` assuming `argv[0]` is the program name.** No `.desktop` file, packaged launcher
  script, or file-association / single-instance configuration exists anywhere in the tree
  (`tauri.conf.json`'s `bundle` block has no `fileAssociations`, and no
  `tauri-plugin-single-instance` dependency is present). There is nothing in this codebase today
  that would invoke the binary with a different argv shape than a direct process exec, where
  `argv[0]` is conventionally the program path on both Linux and Windows. Sound as of this range;
  worth re-checking if a `.desktop` file or single-instance plugin is added later.
- **i18n.** No hardcoded user-facing strings are produced anywhere in `startup_files`,
  `startup_files_command`, or `useStartupFiles.ts` — the path either opens a file through existing,
  already-i18n'd UI flows or does nothing.
