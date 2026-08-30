# Gate 2 — L10: the N1b evidence chain

**Question:** does the committed instrument actually measure what the reports and BACKLOG claim it measured?

## What was checked

- `docs/reports/n1b-sessanta-corse.md` in full, against the working tree at `eca9806` (confirmed with `git diff eca9806 -- docs/reports/n1b-sessanta-corse.md`: no diff, so the file is unchanged since GATE_HEAD).
- `e2e/scripts/n1b-load-probe.js` in full (added in `3657241`, unchanged since).
- `BACKLOG.md`'s N1b entry at `eca9806` (`git show eca9806:BACKLOG.md`) — line numbers below are confirmed against that commit and match the current working tree, since the working tree's uncommitted edits to `BACKLOG.md` touch the M2.0 section only, not N1b.
- `git log --all --diff-filter=A --name-only` for the cited probe filename.
- `e2e/scripts/close-gate-check.js`, `docs/reports/gate2-battery-baseline.md`, `docs/reports/n2b-collaudo-reale.md`, `WORKFLOW.md:80`, and `package.json`'s script table, as cross-checks for the hunt list's other items.

## Findings, most severe first

### 1. [Serious] The six-stream battery that drove the whole diagnosis has no way to tell "ran and didn't crash" from "never ran"

`docs/reports/n1b-sessanta-corse.md:11-16` (the sequential battery) has columns `runs | reached the end | non-zero exit | signal | core dump`. `docs/reports/n1b-sessanta-corse.md:33-38` (the six-stream battery, the one that produced "2 in 30 on save, 0 in 30 on discard" and therefore the entire "load is a condition of the defect, save is not special" conclusion the fix is built on) has only `runs | SIGSEGV | core dump` — no "reached the end" / done column.

`e2e/scripts/n1b-load-probe.js:118-119` swallows every exception silently (`catch {}`), and `phase` is the only signal for how far a run got — `"start"`, `"window"`, `"dirty"`, `"close"`, `"dialog"`, `"answer"`, `"exit"`, or `"done"`. A run whose `clickDialogButton` (`:50-57`) missed the button under six-way X11 focus contention (each of six streams calling `focusWindow` then `clickAt` concurrently, competing for the one X focus) would time out at `phase: "answer"` or `"exit"` and print `{phase: "answer", exit: null, signal: null, ...}` — indistinguishable, in the missing column, from a run that reached `"done"` and simply did not crash. The same is true of a run that never found the toplevel or the dialog at all under load.

Without that column for this specific table, "0 in 30" on discard and the "30" denominator for save cannot be told apart from "fewer than 30 runs actually exercised the close path, and none of the ones that did happened to crash." This is exactly the failure mode the plan names as a finding: _"an instrument that cannot distinguish 'the branch ran and did not crash' from 'the branch never ran'."_

The later table added in `2b31f14` (`docs/reports/n1b-sessanta-corse.md:67-71`) does add a "60 done" figure for both the first and delivered binary's six-stream batteries — so the gap was partially closed for the post-fix verification runs, but the original diagnostic battery that established the save/discard split and pointed the fix at "save is slow, load lengthens the delay" (the causal story the fix in `2b31f14` was built on) still has no such record.

### 2. [Serious] BACKLOG's own AC for N1b is not met by the evidence, and N1b is checked done anyway

`BACKLOG.md:110` (at `eca9806`, matches working tree) states N1b's second acceptance criterion as: _"AC: thirty sequential runs of `pnpm e2e:close-gate` stay clean, and no assertion in it is weakened."_ This is a criterion distinct from and in addition to the retired sequential-probe criterion at `:109`, and it is written as required, not as future work.

The evidence for the **delivered** binary — the one the checkbox at `BACKLOG.md:106` (`- [x] **N1b ... (fixed 2026-08-30)**`) certifies — is **three** sequential runs, not thirty: `docs/reports/n1b-sessanta-corse.md:70` ("`sequential pnpm e2e:close-gate` | 30 green, 0 red | **3 green, 0 red**", first binary vs. delivered binary), and `BACKLOG.md:112`'s own status line says the same thing in different words: _"close gate 12/12 three times"_ — three runs of the 12-assertion check, not thirty. Only the _pre-fix_ binary got the full thirty sequential runs (the "first binary" column). The AC's assertion-count requirement ("no assertion in it is weakened") is satisfied — `git diff f0b0058 eca9806 -- e2e/scripts/close-gate-check.js` shows no `check(...)` calls added, removed or changed, and `EXPECTED_CHECKS` stays 12 — but the run-count requirement is not: the AC needed thirty sequential clean runs on the binary being shipped, and the evidence on record is three.

This is the plan's own example of a finding: _"a criterion declared met by weaker evidence than it names."_ CLAUDE.md §5.4 ("never fake a pass ... if a test is wrong, say so explicitly and fix it as its own change") and §9 (state what was verified vs. assumed) both apply: the checkbox at `BACKLOG.md:106` reads as "the written AC was met," and for this half of it, it was not.

### 3. [Minor] The named probe file doesn't exist, and neither does whatever ran it sixty times

`docs/reports/n1b-sessanta-corse.md:9` names the instrument `n1b-branch-probe.mjs`. `git log --all --diff-filter=A --name-only` finds no file by that name anywhere in the repo's history. The committed script is `e2e/scripts/n1b-load-probe.js`, added whole in `3657241`. BACKLOG's own AC at `:109` correctly names `e2e/scripts/n1b-load-probe.js`, so the closing criterion is written against the file that actually shipped — the mismatch is confined to the report's prose.

More significant than the filename: **the orchestration that ran the probe sixty times — sequentially, then across six concurrent streams — is not in the tree at all.** `n1b-load-probe.js` runs exactly one app instance for one branch and prints one JSON line (`:130-138`); nothing commits a script or `package.json` entry that spawns it in a loop, spawns six of them concurrently, collects the JSON lines, or computes the "2 in 30" / "0 in 60" figures from them. `package.json`'s script table (`e2e:build`, `e2e:shutdown`, `e2e:close-gate`, `e2e:wayland`, `e2e:scale`) has no N1b entry, and no other file in the diff (`git diff --diff-filter=A --name-only f0b0058 eca9806`) matches `aggreg`, `batter`, or similar. Whatever counted "done" vs. not, and whatever decided the six streams' concurrency and collection, exists only as a command someone typed at a terminal — which means finding 1 above (whether the six-stream table filtered on `phase === "done"`) cannot be resolved by reading the repository, at all. This is the plan's _"a claimed measurement whose apparatus is not in the tree"_ case, and it compounds finding 1: not only is the "reached the end" column missing from the six-stream table, the code that would have produced it isn't committed either.

### 4. [Minor, suspicion] `killGroup` in the probe's `finally` block can end a run before it would have crashed or exited on its own

`e2e/scripts/n1b-load-probe.js:120-128`:

```js
} finally {
  try {
    if (processGroupMembers(pgid).length > 0) {
      killGroup(pgid);
    }
  } catch {
    // Teardown must not rewrite the result.
  }
}
```

`killGroup` (`e2e/lib/proc.js:58-63`) sends `SIGKILL` to the whole process group. This runs unconditionally on every exit from the `try` block — including a run that timed out at `phase: "exit"` because the app hadn't self-terminated within the 20 s wait at `:116`, rather than one that crashed or exited cleanly. A `SIGKILL` delivered externally produces no `coredumpctl` entry the way a `SIGSEGV` does, so a run that was about to crash on its own, but hadn't yet by the 20 s mark, would be recorded as a bare timeout instead of a crash, and its process group would be gone before anyone could tell which it was.

I'm flagging this as a suspicion, not a certain defect, per the brief's own instruction to label what I cannot demonstrate: BACKLOG describes the race as landing "one loop iteration after" `window.destroy()`/`window.close()` (`BACKLOG.md:106`), i.e. on the order of a single GTK main-loop iteration, not tens of seconds, and the wait at `:116` gives it 20 s. So in the batteries actually reported, this window is unlikely to have been the reason any run read "0 SIGSEGV" — but the design leaves no way to be sure a slow/loaded run near the timeout boundary wasn't quietly killed instead of allowed to finish crashing or exiting, and that is a real gap in an instrument this report leans its central claim on.

## Hunt items checked and found sound

- **The button-click geometry** (`n1b-load-probe.js:50-57`, `buttonWidth = 96`, `slot = {save: 2, discard: 1}`) is the same constant `close-gate-check.js:117-125` uses for the identical native GTK dialog, and that script _asserts_ (not just observes) the click landed on the right button. `docs/reports/gate2-battery-baseline.md:17` records `close gate check passed (12/12 checks)` at GATE_HEAD, and `BACKLOG.md:112` records "close gate 12/12 three times" on the delivered binary — both save and discard branches, multiple runs, all green under actual assertions. That's affirmative evidence the constant is correct today, not just the probe's own silence about it. I can't rule out label/theme-driven width drift in principle (GTK sizes buttons to their label text and nothing pins font/theme in the harness), but there's real assertion-backed evidence against a systematic miss, so I'm not raising this as a live finding.
- **The arithmetic**: `2/30 ≈ 0.0667` per-run rate, `(28/30)^60 ≈ e^(60·ln(28/30)) ≈ e^-4.14 ≈ 1.59%`, which rounds to "about one time in sixty" as both `n1b-sessanta-corse.md:73` and `BACKLOG.md:112` say. Checks out.
- **The "after" battery ran under the same load condition as the "before" one**: `BACKLOG.md:112` explicitly states the delivered-binary judgment was "60 save-branch runs in six concurrent streams," matching the pre-fix condition. No discrepancy there. (The independence of trials _within_ a six-stream battery is a separate, softer concern — shared machine load during concurrent teardown isn't strictly i.i.d. — but the report doesn't claim otherwise, and I have no way to demonstrate it changed the result, so I'm not raising it as a finding.)
- **WORKFLOW §4c's discrimination rule ("check the build's exit status explicitly")**: this rule's origin is documented, not asserted from nowhere. `docs/reports/n2b-collaudo-reale.md:71,79` records that the `x11egl` removal experiment referenced in `e2e/scripts/wayland-attach-check.js:141-146` hit exactly the silent-`&&`-swallows-a-failed-build bug once, was caught, and the rule was written into `WORKFLOW.md:80` as a direct result. That's the strongest form of "honoured" — a documented failure and fix, not just a claim. `n1b-sessanta-corse.md` itself doesn't restate a build-exit-status check inline for the N1b batteries, but I found no evidence one was skipped either, and the hunt item asks specifically to check the `x11egl` case as the comparison point, which holds up.
- **`n1b-branch-probe.mjs` vs. `n1b-load-probe.js` in BACKLOG itself**: BACKLOG's AC at `:109` correctly cites the committed filename. Only the report's prose (`n1b-sessanta-corse.md:9`) uses the nonexistent name, so N1b's closing criterion in BACKLOG is not itself broken by the mismatch — only the report's traceability is (folded into finding 3).
