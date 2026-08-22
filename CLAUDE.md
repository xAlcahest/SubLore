# CLAUDE.md — Sublore

**Sublore — translation memory for subtitles.**
Local-first desktop app for translating subtitles across whole series with terminology consistency. **Platform policy: v1 targets Windows and Linux. macOS is deferred until further notice by owner decision** — do not build, test, or debug macOS-specific paths, but never introduce a dependency or design that would block a later macOS port; every component in §2 is mac-compatible and must stay that way. Whisper transcription is a commodity we wrap; the product is the memory: a persistent termbase and translation memory that follows the translator through every episode, plus QA that flags every line where an approved term was not used.

The owner directs this project through coding agents and verifies behavior end-to-end. He does not hand-review code. Every rule in this file exists to make the codebase safe to build under that model. Read this whole file before writing anything.

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

If a task seems to require expanding this scope, stop and ask the owner. Do not interpret ambiguity as permission.

## 2. Architecture (decided — do not relitigate)

- **Shell:** Tauri 2. **Core logic:** Rust. **UI:** TypeScript + the framework chosen in the repo (follow what exists; do not introduce a second one).
- **Video:** libmpv embedded. Never implement custom decoding/rendering paths; if libmpv can't do it, the feature waits.
- **ASR:** whisper.cpp as a sidecar process. GPU via Vulkan where available, CPU fallback always working. CUDA is never a hard dependency.
- **Storage:** SQLite, one database file per project. Schema changes require a migration with an automated round-trip test (old db → migrate → verify).
- **Alignment:** word-level timestamps from whisper.cpp; forced-alignment improvements are post-1.0.

Rationale exists for each of these; the owner approved them. Proposing a different stack is a conversation with the owner, not a refactor.

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

## 5. How work is verified (this section overrides habit)

The owner verifies behavior, not code. Therefore:

1. **Every feature ships with acceptance criteria written first**, in plain language, as observable behavior: "open fixture X, run QA, lines 12 and 40 are flagged, line 7 is not." If acceptance criteria can't be written as observable behavior, the feature is not specified yet — go back to the owner.
2. **Automated E2E/behavioral tests are the primary test layer.** Unit tests support them; they never replace them. A green unit suite with no behavioral coverage is not done.
3. **Test fixtures are real-shaped:** actual SRT/ASS files with CRLF/LF variants, BOM, overlapping cues, non-Latin text, malformed lines. A fixture folder is part of the repo and grows with every bug fixed (regression fixture per bug).
4. **Never fake a pass.** No weakening assertions, no skipping tests, no adjusting expected values to match broken output. If a test is wrong, say so explicitly and fix it as its own change.
5. Cross-platform CI matrix (Windows, Linux) must be green before any release tag; macOS joins the matrix when the owner re-activates it. "Works on my platform" does not exist here.
6. Review pipeline: significant changes go through Claude Code's built-in code review (`/review`) before being presented as complete; findings are fixed or explicitly acknowledged in the PR description.

## 6. Code quality

Follow the code-quality skill in full. Sublore-specific points:

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
- Conventional, plain commit messages. No Co-Authored-By or AI attribution trailers unless the owner explicitly approves (gcp skill applies).
- No new dependencies without stating in the PR: what it does, why stdlib/existing deps can't, license, maintenance status. GPL-compatibility is mandatory in the open repo.
- Never commit: secrets, keys, model binaries, user media, generated artifacts.
- If instructions conflict (this file vs a prompt vs a skill), this file wins for scope, safety, and architecture; the owner's live instruction wins over everything after he confirms the conflict.

## 9. Voice and honesty

- The app's UI copy is plain and short. English source strings; i18n-ready from the start (no hardcoded user-facing strings).
- Marketing and docs never overclaim: transcription accuracy is Whisper's, and we say so. The claim we own is consistency: "your terminology, enforced across the whole series."
- When reporting status to the owner: state what was verified by running the app, what is only assumed, and what remains untested. Unverified work is presented as unverified. This is the single most important rule in this file.
