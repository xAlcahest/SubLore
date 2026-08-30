# Gate 2 — Wave 3 consolidation

Files changed: `e2e/scripts/startup-args-check.js` (rewritten as the merge),
`e2e/scripts/argv-startup-check.js` (deleted), `package.json`, `.github/workflows/ci.yml`,
`e2e/README.md`, plus one formatter fix in `docs/reports/gate2-fix-harness.md` that is named in §5.
Nothing under `src-tauri/`, `src/`, `e2e/lib/` or any other implementer's script was touched.

Everything below that says "passed" was run on Linux (Fedora, Xvfb, X11). The CI steps added here
have never executed on a GitHub runner; that is stated as unverified in §4, not reported as green.

---

## 1. The two argv checks are now one

**Kept: `e2e/scripts/startup-args-check.js`. Deleted: `e2e/scripts/argv-startup-check.js`.**

The name is the reason. Every other script in the directory is named for its subject and then its
aspect — `close-gate-check`, `scaled-surface-check`, `wayland-attach-check`, `webview-paint-check` —
and "startup args" is the subject in the same plain register the rest of the repo uses, while "argv"
is the implementation's word for it. The surviving script also covers more than argument parsing (it
asserts the launch survives and the close is clean), so the broader name is the truer one.
`gate2-fix-lib.md` had already proposed `e2e:startup-args` as the script entry, so the package.json
name follows from the file name rather than being invented here.

### The union, assertion by assertion

`EXPECTED_CHECKS = 6`. Nothing was dropped, weakened or retargeted.

| #   | Assertion                                                                      | From                       |
| --- | ------------------------------------------------------------------------------ | -------------------------- |
| 1   | the app came up with an argument that is not valid Unicode on its command line | `startup-args`             |
| 2   | closing it exits 0 with nothing left alive                                     | `startup-args`             |
| 3   | the argument it could not carry is named in the log                            | both, in the stronger form |
| 4   | the subtitle named beside it was still the one taken                           | `startup-args`             |
| 5   | a subtitle whose name starts with a dash is taken, not dropped                 | `argv-startup`             |
| 6   | a path that is not there is named in the log                                   | `argv-startup`             |

**The one overlap, and which half survived.** Both scripts asserted that the unreadable argument
reaches the log. `startup-args` matched the substring `not valid Unicode` anywhere in the file;
`argv-startup` matched `/ignored .*rie\.srt \(not valid Unicode\)/`, which also proves the app named
_which_ argument it dropped. The stronger one won, and was then strengthened once more into an exact
match on the whole line, `command line: ignored <path with U+FFFD> (not valid Unicode)`, which the
merged script can build itself because `to_string_lossy` is deterministic. The weaker form is gone
because it is implied by the stronger, not because the count wanted tidying.

### Two launches, and why the count is 6 rather than 5

`startup_files` keeps only the first subtitle it accepts (`files.subtitle.get_or_insert_with`), so
"the good subtitle beside the bad argument was taken" and "the dash-named subtitle was taken" cannot
both be observed in one run: whichever comes second is discarded by the app, correctly. Folding them
into a single assertion on a single log line would have merged away one of the two claims, which the
brief forbids. The merged script therefore does two sequential launches, each with its own
`XDG_DATA_HOME` so neither log can answer for the other, and the first is fully reaped before the
second starts (`findToplevel` throws when two windows match, so two live instances would fail the run
rather than poison it).

### One assertion had to be rebuilt to be able to fail

`argv-startup`'s author had deliberately _not_ counted "the window came up", on the grounds that a
`check` on a window `waitForWindow` had already waited for can only be true — WORKFLOW §2.5b's ban on
assertions on a constant. That reasoning was right about their code and wrong as a reason to lose the
assertion, so the wait was rebuilt instead: `windowOrReason` returns `{toplevel: null, reason}` rather
than throwing, and check 1 is a real assertion that fails when the launch dies on its own command
line. It short-circuits on process exit, so a regression fails in about a second rather than after a
30 s timeout.

**Proved it can fail**, without rebuilding Rust: `CARGO_TARGET_DIR` was pointed at a temp directory
whose `debug/sublore` is `#!/bin/sh\nexit 101`, the exit status the pre-fix panic produced.

```
startup args check failed: the app came up with an argument that is not valid Unicode on its command line
the app exited (code 101, signal null) before its window appeared. Status 101 is a panic: the
argument that is not valid Unicode cost the whole launch, which is gate 2 `src-tauri/src/lib.rs:75`.
```

**What I did not re-prove.** Checks 3 to 6 are unchanged in kind from the two originals, whose
authors ran their own discrimination experiments against them (`gate2-fix-lib.md`). Re-running one
here would mean rebuilding the Rust binary, and WORKFLOW §4c puts `pnpm e2e:build` last; I did not do
it. Their falsifiability is inherited, not re-measured by me.

### The passing run asked for

```
cd /home/alcahest/git/SubLore && xvfb-run -n 880 -s "-screen 0 1024x700x24" node e2e/scripts/startup-args-check.js
```

```
[INFO] command line: video=None, subtitle=Some("/tmp/sublore-e2e-argv-bad-N3o5fq/beside-it.srt")
[WARN] command line: ignored /tmp/sublore-e2e-argv-bad-N3o5fq/s?rie.srt (not valid Unicode)
  ok  the app came up with an argument that is not valid Unicode on its command line
  ok  closing it exits 0 with nothing left alive
  ok  the argument it could not carry is named in the log
  ok  the subtitle named beside it was still the one taken
[INFO] command line: video=None, subtitle=Some("-export.srt")
[WARN] command line: ignored s?rie.srt (not valid Unicode)
[WARN] command line: ignored epsiode.srt (not a file on disk)
  ok  a subtitle whose name starts with a dash is taken, not dropped
  ok  a path that is not there is named in the log
startup args check passed (6/6 checks)
```

(The `?` above is U+FFFD; the log carries the replacement character, which is what check 3 matches.)

The binary it ran against is a `pnpm e2e:build` one: `target/debug/sublore` embeds the current
`dist/assets/index-BSEmJyV9.css`, and is newer than every file under `src-tauri/src` and `src`. No
`cargo build` or `cargo test` was run after it, per WORKFLOW §4c.

---

## 2. Wired in

`package.json`, following the existing `e2e:` naming:

```
"e2e:close-gate-late-edit": "node e2e/scripts/close-gate-late-edit-check.js",
"e2e:startup-args": "node e2e/scripts/startup-args-check.js",
```

`.github/workflows/ci.yml`, in the `e2e` job beside the other behavioural checks, at the same screen
size as its neighbours:

- `Late-edit gate test` → `pnpm e2e:close-gate-late-edit`
- `Startup args test` → `pnpm e2e:startup-args`

**One correction to the brief.** `scaled-surface-check.js` was not unwired: `gate2-fix-env.md`'s
implementer added both the `package.json` entry and the CI step for it during wave 3, which is the
original `package.json:19` / `ci.yml:196` row closed at its own site. `e2e:wayland` and `e2e:webview`
also already existed as package scripts; what was missing for those two was a CI decision, which §3
records. So the wiring left to do was two package scripts and two CI steps, not five of each.

Both new steps were run locally through the package scripts, so the wiring itself is proved and not
just the file paths:

```
xvfb-run -n 883 -s "-screen 0 1280x1024x24" pnpm e2e:startup-args        → 6/6 checks
xvfb-run -n 884 -s "-screen 0 1280x1024x24" pnpm e2e:close-gate-late-edit → 8/8 checks
xvfb-run -n 885 -s "-screen 0 1280x1024x24" pnpm e2e:scale                → 5/5 checks
```

`e2e:scale` is included because it is the neighbour step and this consolidation touches the file it
runs from; it is still green.

---

## 3. Deliberately not in CI, and recorded as such

A comment block now sits at the end of the `e2e` job naming both omissions and their reasons, so
"not in CI" is a decision in the workflow file rather than an absence:

- **`pnpm e2e:webview`** needs `/sys/module/nvidia` for the branch under test to be taken. On
  `ubuntu-latest` the module is absent, so the check would assert `not applied` and measure only that
  llvmpipe paints something — coverage the smoke test already has. It would be green while proving
  nothing about the mitigation, which is the failure mode WORKFLOW §4c calls the worst available. It
  also needs ImageMagick's `import`, which the `e2e` job does not install today. Its own header had
  already reached the same conclusion; the workflow now says so where a reader of CI will find it.
- **`pnpm e2e:wayland`** needs a real Wayland socket. Without one mpv falls back to X11 and the check
  would pass for the wrong reason, which is why it refuses to run rather than skipping.

Neither is silently skippable: both fail loudly on a missing prerequisite. Both stay owner-machine
checks under WORKFLOW §4c, and neither has a guard that would make it meaningful on a GitHub runner —
adding a "skip if no NVIDIA module" branch would put a step in CI that is green on every push while
never once executing the code it names, which is worse than the recorded omission.

---

## 4. What is verified and what is not

- **Verified on Linux, by running it:** the merged check (6/6), the late-edit check (8/8), the scale
  check (5/5), all three through the package scripts under Xvfb; check 1 of the merged script failing
  when the app dies on its command line; `npx prettier --check .` clean over the whole repo;
  `npx eslint .` clean; `.github/workflows/ci.yml` parses as YAML and its `e2e` job lists the two new
  steps in order.
- **Not verified:** that the two new CI steps pass on a GitHub runner. They have run only on this
  machine. A runner differs in GPU stack, timing and installed packages, and the late-edit check
  drives a GTK dialog by clicking estimated button coordinates — the closest thing here to a step that
  could behave differently there. The first push is what will show it.
- **Not verified:** anything on Windows. Neither new step runs in the `check` job, and no behaviour in
  this consolidation has been executed on Windows.
- **Not re-measured:** the falsifiability of merged checks 3 to 6, per §1.

---

## 5. Noticed, and one thing outside the task that I did change

**Changed:** `docs/reports/gate2-fix-harness.md` failed `prettier --check`, on one italics marker
(`*before*` → `_before_`) and nothing else. `pnpm format:check` runs over the whole repo in the
`check` job on both platforms, so leaving it would have turned CI red on the next push and defeated
the point of wiring the new steps in. It is a formatter's emphasis character, not content: no word,
number or claim in that report moved. Naming it here because it is another implementer's file and a
silent edit to one is not acceptable, whatever its size.

**Not changed, for the orchestrator:**

1. `e2e/README.md`'s tool paragraph said ImageMagick "is deliberately not used", which stopped being
   true when `webview-paint-check.js` and `real-session-check.mjs` started capturing with
   `import -window`. That sentence sits in the file this task owns, so it is corrected: nothing that
   runs in CI uses ImageMagick, and the two things that do are named along with why a root grab
   cannot replace it. If the intent was that ImageMagick must never be a harness dependency at all,
   that is an owner ruling and the two scripts, not the README, are what would have to change.
2. The stray untracked file named `--help` at the repo root, reported by two wave-3 implementers, is
   still there. It must not be committed.
3. `docs/reports/gate2-register.md` still lists `package.json:19` and `.github/workflows/ci.yml:196`
   as `open`. Both are closed now — the `e2e:scale` half by `gate2-fix-env.md`, the wider "no job runs
   the new checks" half by this consolidation. Updating the register's status column is the
   orchestrator's, not mine.

---

## 6. e2e/README.md

Seven rows added to the spec table, one line each, each saying what the script proves rather than how:
`close-gate-late-edit-check.js` (8 checks), `startup-args-check.js` (6), `scaled-surface-check.js` (5),
`webview-paint-check.js` (5), `wayland-attach-check.js` (4), and the two probes,
`n1b-load-probe.js` and `real-session-check.mjs`, marked "probe, asserts nothing" so no reader can
quote a probe's output as a pass.

A paragraph under the table says which rows run in CI and which do not, and why the two that do not
cannot be made meaningful on a runner. The local-run block gained the three Xvfb commands that now
have package scripts, and a second block for the two owner-machine checks — `pnpm e2e:wayland`
without an `xvfb-run` wrapper, since wrapping it in one is exactly what would break it.
