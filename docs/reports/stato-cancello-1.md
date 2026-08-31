# Gate 1 — closed, 2026-08-30

## What is on main

Three commits since the last status.

| commit    | what                                                                   |
| --------- | ---------------------------------------------------------------------- |
| `8a560b2` | The owner's thirteen decisions, the platform policy, and the NOW block |
| `6d6d3ab` | N1, the close gate: unsaved edits are never dropped silently           |
| `d224f3c` | N2, the video surface: visibility derived, never set                   |
| `77ff7bb` | The M2.0 preparation, written in parallel and **not verified**         |

**Verified on Linux**, on the last battery: `cargo fmt` clean, `cargo clippy -D warnings` exit 0 with zero warnings, 495 Rust tests, 8 E2E spec files with 33 checks, the shutdown check 5/5, the close gate check 12/12, eslint and prettier clean. Windows compiles and is not verified; that is MW's job and it gates any release.

## What the gate found

Three delegated lenses over N2: **7 blockers, 15 serious, 25 minor. All fixed.**

The two worth remembering, because both were defects the corrections themselves introduced:

- The fix for the first review pass made the surface visible **at startup with no video open** — an opaque slab over the empty stage, worse on Windows where the surface paints its own background. Caught by measuring the running binary before and after, not by reading the diff.
- The assertion that was supposed to prove the clock was frozen **could not fail**: `waitFor` returns on its first evaluation, so it compared a reading with itself milliseconds later. That is the same class of defect the previous pass had blocked, rewritten inside its own correction.

That is what the gate is for, and it is why per-task self-review does not replace it.

## What opens tomorrow

1. **N2b** — libmpv does not attach to the X11 surface inside a Wayland session. The app works on the owner's own machine only because the harness scrubs the environment; the product itself does not. Foundation defect on the declared primary platform. Its test must start from a session with `WAYLAND_DISPLAY` present and assert the real attachment: mpv's child window **and** a visible frame.
2. **Decision 1** — the video surface hides for HTML layers and comes back when they close. N2 built the machine it needs: visibility is already derived from a single state, and decision 1 adds the third input.
3. **Gate 2** — over N2b and decision 1 together, with one lens declared suspect in advance: the interaction between them. N2b changes how mpv attaches to the surface, decision 1 hides and shows that surface on every menu, and the two meet on every menu opened over a playing video. Neither task exercises that on its own.
4. **The owner's manual checklist**, on the built app. Nothing on M2.0 starts before it.

## Owed, and not done

**The M2.0 breakdown has not been read end to end.** 1124 lines, ten tasks, written by a delegated agent that passed two adversarial critiques and then died without its closing report. What was checked is the index, the owner-questions section, the task count and the left-open findings: the shape, not the content. The document states that it applied 12 blocking and 23 serious findings, and nobody has verified that statement. Reading it in full is a prerequisite for starting M2.0, and `BACKLOG.md` carries the same warning.

The one owner question the preparation surfaced is answered and needs nothing: "video and waveform side by side" cannot be shown at M2.0 because no audio provider exists before M2.4, and the plan's own answer stands — a panel with no provider is absent rather than empty, because an empty placeholder is dead UI and CLAUDE.md §6 rules it out.

## Small debts carried forward

- The saturation threshold that decides "there is a picture" is measured on Fedora with software rendering, not on the CI runner. If that stack renders the bars flatter the test's precondition fails there rather than passing wrongly, but the number itself is unverified off this machine.
- `close-gate-check.js` still uses fixed waits between opening a file and double-clicking a row: without a DOM there is nothing observable to wait on. M2.0 should replace them.
- The remove branch of the selection remap and the uncommitted-inline-editor text at close both remain uncovered, each recorded where it belongs.
