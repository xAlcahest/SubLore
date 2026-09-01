# Contributing to Sublore

**Sublore — translation memory for subtitles.**
Local-first desktop app for translating subtitles across whole series with terminology consistency. Whisper transcription is a commodity we wrap; the product is the memory: a persistent termbase and translation memory that follows the translator through every episode, plus QA that flags every line where an approved term was not used.

**Platform policy (owner decision 2026-08-29, supersedes the previous one):**

- **Linux is the primary platform for development and verification.** Behavioural work is built and proved here first.
- **Windows compiles in CI on every push** and must never be allowed to break. Compiling is not verifying: no Windows behaviour is claimed until it has been run.
- **Full Windows activation is its own mandatory milestone**, covering the E2E backend with native input and window inspection, platform hardening, and the owner checklist run on Windows. **It is required before any sale or public release.** No release goes out on Linux alone.
- **macOS stays deferred** until further notice — do not build, test, or debug macOS-specific paths, but never introduce a dependency or design that would block a later macOS port; every component in §2 is mac-compatible and must stay that way.

Sublore is verified by behaviour rather than by code review: what proves a change is the app doing the thing, on a real file, where someone can watch it happen. The rules below are what stands in for a reviewer, which is why they are specific and why they are not renegotiated one change at a time.

---

## 1. Product definition

### v1.0 scope (sellable minimum)
- Import/export: SRT, ASS, VTT. Round-trip must be lossless for supported fields.
- Video playback with waveform, via embedded libmpv.
- Local transcription via whisper.cpp sidecar (word-level timestamps).
- Side-by-side source/target editing view.
- **Termbase (pro):** per-project glossary with terms, approved renderings, notes. QA pass highlights every target line where a source term appears but its approved rendering does not. This feature is the product. It ships in v1, never deferred.
- **Translation memory (pro):** exact + fuzzy matching over all lines in the project, across episodes.
- Offline license check for pro modules (see §4).

### Explicit non-goals for v1 (do not build, do not scaffold "for later")
- Karaoke, advanced ASS typesetting, animation.
- Speaker diarization.
- Built-in LLM translation. At most: bring-your-own-key hook, off by default.
- Cloud sync, accounts, telemetry, auto-update phone-home. Sublore never talks to the network except: optional model download, optional BYOK calls, explicit update check.
- Mobile, web.

Work that only fits by widening this scope is a scope decision, and scope decisions are settled with the owner before the code is written. An ambiguous ticket is not permission.

## 2. Architecture (decided)

- **Shell:** Tauri 2. **Core logic:** Rust. **UI:** TypeScript + the framework chosen in the repo (follow what exists; do not introduce a second one).
- **Video:** libmpv embedded. Never implement custom decoding/rendering paths; if libmpv can't do it, the feature waits.
- **ASR:** whisper.cpp as a sidecar process. GPU via Vulkan where available, CPU fallback always working. CUDA is never a hard dependency.
- **Storage:** SQLite, one database file per project. Schema changes require a migration with an automated round-trip test (old db → migrate → verify).
- **Alignment:** word-level timestamps from whisper.cpp; forced-alignment improvements are post-1.0.

Each of these was chosen for a reason and the choice holds. A different stack is a conversation with the owner, not a refactor.

## 3. Data safety (hard rules, no exceptions)

1. **Source media is read-only.** Sublore never writes, moves, renames, remuxes, or "fixes" the user's video or audio files. No feature may require it.
2. **Subtitle writes are atomic:** write temp file, fsync, rename. Never truncate-then-write in place.
3. Before overwriting any user subtitle file, keep a timestamped backup in the project folder (rolling, small cap). Deleting backups is a user action, never automatic cleanup logic.
4. The project database is append-safe: crashes may lose the last operation, never the database.
5. No feature performs bulk writes across arbitrary user folders. Sublore touches only: files the user explicitly opened, and its own project folder.

Failure mode budget: a Sublore bug may cost the user annoyance, never data. Any design that could violate this is rejected regardless of how useful the feature is.

## 4. Open-core boundary

- **Open (this repo, GPL-3.0):** editor, playback, waveform, formats, timing, whisper integration, project files.
- **Closed (separate private repo, built as dynamically loaded modules):** termbase + QA, translation memory, batch processing.
- The open core must be fully useful without the pro modules: it is the free product, not a crippled demo.
- The license check is offline (signed key file), gentle, and lives in the closed modules. **No license logic, feature flags, or "isPro" branches in the open repo.** The open core exposes a stable module-loading interface and nothing more.
- Never commit private-module code, keys, or key-generation tooling to the open repo. Check paths before every commit.

## 5. How work is verified

Behaviour is what gets checked here, not code. Therefore:

1. **Every feature ships with acceptance criteria written first**, in plain language, as observable behavior: "open fixture X, run QA, lines 12 and 40 are flagged, line 7 is not." If acceptance criteria can't be written as observable behavior, the feature is not specified yet, and specifying it comes before building it.
2. **Automated E2E/behavioral tests are the primary test layer.** Unit tests support them; they never replace them. A green unit suite with no behavioral coverage is not done.
3. **Test fixtures are real-shaped:** actual SRT/ASS files with CRLF/LF variants, BOM, overlapping cues, non-Latin text, malformed lines. A fixture folder is part of the repo and grows with every bug fixed (regression fixture per bug).
4. **Never fake a pass.** No weakening assertions, no skipping tests, no adjusting expected values to match broken output. If a test is wrong, say so explicitly and fix it as its own change.
5. **Behavioural verification happens on Linux; Windows compiles.** The E2E suite drives the app through X11 and runs on Linux only, so today it proves Linux behaviour and nothing else. The Windows `check` job must stay green on every push, and a green compile is never reported as a working feature. The full matrix — behavioural suite green on Windows too — is the exit condition of the Windows activation milestone, and that milestone gates any sale or public release. macOS joins when the owner re-activates it. "Works on my platform" does not exist here; neither does "it compiled". Check the Windows build before pushing rather than after: `rustup target add x86_64-pc-windows-gnu` once, then `cargo clippy --target x86_64-pc-windows-gnu -p sublore --all-targets -- -D warnings` compiles the `cfg(windows)` paths on Linux in seconds. It is not the MSVC toolchain CI uses, so it proves types and not linking, which is enough to catch the whole class of "this half was never compiled".
6. Review pipeline: significant changes go through a review pass before being presented as complete; findings are fixed or explicitly acknowledged in the PR description.

## 6. Code quality

The usual standards apply in full. The Sublore-specific points:

- Before writing: study existing patterns, reuse what exists, trace the blast radius of changes to shared code.
- Minimum code that solves the problem. No dead code, no speculative abstractions, no config for hypothetical futures.
- Comments: max 1–2 lines per guard/block, reference the issue number; longer reasoning goes in the PR description, never inline.
- Error handling at boundaries (file I/O, sidecar, IPC, user input); trust internal invariants.
- TypeScript: no `any` unless unavoidable. Rust: no `unwrap()` outside tests; errors surface to the UI as actionable messages, never silent logs.
- Public interfaces (IPC channels, module-loading API, project schema) are stable: changing them means updating every consumer in the same PR and calling it out.

## 7. Performance budgets (measured, not vibed)

- Cold start to interactive: < 2 s on a mid-range 2020 laptop.
- Idle memory (project open, video paused): < 400 MB excluding whisper model memory.
- UI stays responsive during transcription: ASR runs in the sidecar, never blocks the main thread; progress is visible and cancellable.
- Opening a 2,000-line subtitle file: < 1 s. QA pass over one episode: < 5 s.

A PR that regresses a budget states it explicitly and waits for owner approval.

## 8. Git and process

- Small, single-purpose PRs. PR description states: what changed, why, and **how to verify it by using the app** (steps a non-coder can follow).
- Conventional, plain commit messages, with no attribution trailers.
- No new dependencies without stating in the PR: what it does, why stdlib/existing deps can't, license, maintenance status. GPL-compatibility is mandatory in the open repo.
- Never commit: secrets, keys, model binaries, user media, generated artifacts.
- If this file conflicts with anything else written down, this file wins for scope, safety, and architecture; a live instruction from the owner wins over everything once he has confirmed the conflict.

## 9. Voice and honesty

- The app's UI copy is plain and short. English source strings; i18n-ready from the start (no hardcoded user-facing strings).
- Marketing and docs never overclaim: transcription accuracy is Whisper's, and we say so. The claim we own is consistency: "your terminology, enforced across the whole series."
- Status is reported honestly: what was verified by running the app, what is only assumed, and what remains untested. Unverified work is presented as unverified. This is the single most important rule in this file.
- **Every behavioural verdict carries its platform.** Write "verified on Linux", never a bare "verified", until the Windows activation milestone lands. A verdict with no platform on it reads as a claim about both, and that claim is false today.
