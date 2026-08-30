# Gate 2b, round two: the two escape hatches and the undocumented one

Files owned this round: `src-tauri/src/main.rs`, `README.md`. Nothing else was touched.
Verified on Linux (Fedora, `/sys/module/nvidia` present). No Windows or macOS claim is made here.

## The rule this round settles

**An empty value means the variable is unset.** The probe decides, exactly as if the variable were
absent. That is what `main.rs` already did by accident of its catch-all arm; it is now stated in the
docstring, at the README, and pinned by a test that fails if the meaning flips.

Why this direction and not the other one:

- The shell makes an empty value cheap by accident. `SUBLORE_WEBKIT_WORKAROUNDS=$SOMETHING_UNSET`,
  a launcher script that exports a variable it never filled, a desktop entry with an empty `Env=`:
  all of these produce a set, empty variable that the user never meant as an instruction. Reading it
  as "unset" costs that user nothing. Reading it as an override silently changes the rendering path
  on a machine that needs the mitigation, which is the blank window of `n2b-collaudo-reale.md`.
- The webkit hatch already has words for both directions (`0/false/no/off`, `1/true/yes/on`), so the
  empty value has no job left to do. It is only the mpv hatch, whose value space is mpv's own
  `gpu-context` names, that pressed the empty string into service as "no pin".

## Findings

### V1 finding 5, first half: the empty string means opposite things in the two hatches

`SUBLORE_WEBKIT_WORKAROUNDS=` leaves the `/sys/module/nvidia` probe deciding (`main.rs:16-24`);
`SUBLORE_MPV_GPU_CONTEXT=` means "leave mpv alone" (`player.rs:66`), which is an override and not
"as if unset".

**Changed (my half).** `nvidia_workarounds_wanted`'s docstring now states the rule instead of leaving
it implicit in the `_` arm: every other value, the empty one included, counts as unset. The README
paragraph says the same thing in the user's words. The behaviour is unchanged, which is the point:
this half of the pair was already right and now says so.

**Proof.** `an_empty_value_decides_exactly_what_no_value_decides` (`main.rs:132`) asserts that the
empty hatch and the absent hatch reach the same decision for both states of the module probe, and
that a blank value leaves the probe in charge. Discrimination run: adding `Some("")` to the
disarming arm made it fail (`left: false, right: true`) along with
`an_unrecognised_value_leaves_the_probe_in_charge`; the edit was reverted and all 7 tests pass again.

**State: half closed, and the half that closes it is not mine.** The two hatches still disagree
until `player.rs` follows: `Some("") => None` at `player.rs:66` has to fall through to the
`WAYLAND_DISPLAY` probe the way `None` does. `player.rs` belongs to another implementer this round,
so I did not reach in. A user who wants no pin at all keeps a way to ask for it, since mpv accepts
`auto` as a `gpu-context` name and forwards straight through the same arm. **This is the one item
that needs another implementer's change before the finding is closed.**

### V1 finding 5, second half: a value that is not UTF-8 prints as "unset"

Both hatches read through `std::env::var(..).ok()`, so a variable holding bytes Rust cannot decode
was reported as absent in the startup line, for a variable the user had set. This is the `OsString`
lesson `startup_files` was rewritten to learn in the same range (`lib.rs:46-47`), applied to the
hatch beside it.

**Changed.** `mitigate_nvidia_webview` reads `std::env::var_os` (`main.rs:51`). Two small functions
sit at the boundary so both halves can be tested without setting a process-wide variable:
`hatch_decision` (`main.rs:30`) decodes for the decision, and a value that does not decode matches no
keyword and leaves the probe in charge; `hatch_report` (`main.rs:37`) prints `unset` only when the
variable really is absent and prints anything else lossily, the way `startup_files` names an argument
it cannot decode. The variable name is now the `WEBKIT_HATCH` const, used both to read it and in the
printed line, so the two cannot drift apart. This mirrors `GPU_CONTEXT_HATCH` in `player.rs`.

**Proof.** Two unit tests: `a_value_that_is_not_utf8_leaves_the_probe_deciding` (`main.rs:148`) and
`the_startup_line_reports_a_set_hatch_as_set` (`main.rs:155`). Discrimination run: restoring the old
`and_then(OsStr::to_str).unwrap_or("unset")` report made the second fail on
`assert_ne!(reported, "unset")`; reverted, both pass.

Proved in the real process as well, launching the binary four times with the variable held as raw
bytes (the decision line is printed before any GTK or webview work, so no display is involved):

```
unset:     sublore: webview workarounds applied (SUBLORE_WEBKIT_WORKAROUNDS=unset, /sys/module/nvidia present)
empty:     sublore: webview workarounds applied (SUBLORE_WEBKIT_WORKAROUNDS=, /sys/module/nvidia present)
not utf-8: sublore: webview workarounds applied (SUBLORE_WEBKIT_WORKAROUNDS=0<U+FFFD>, /sys/module/nvidia present)
off:       sublore: webview workarounds not applied (SUBLORE_WEBKIT_WORKAROUNDS=off, /sys/module/nvidia present)
```

The line's shape is unchanged, so `webview-paint-check.js`'s `DECISION` regex and its three
assertions on the captured value still match: `unset` for the absent case, `0` for the disarmed one.

**State: closed for `main.rs`.** `player.rs:209` still reports through `var(..).ok()` and has the
same defect; that file is another implementer's this round and I did not touch it.

### V2 audit row `main.rs:20`, "not closed": the hatch is documented nowhere a user will look

**Changed.** A paragraph in `README.md`, beside the `SUBLORE_FORCE_PANIC` one, in the section where
the app's other environment variable already lives. It gives the variable name, what the app does on
its own, both sets of accepted values with what each is for, that case and spaces are ignored, that
an empty or unrecognised value means the same as not setting it, that the app prints the decision it
took, and that unlike `SUBLORE_FORCE_PANIC` this one is read in release builds too.

**Proof.** `grep -n SUBLORE_WEBKIT_WORKAROUNDS README.md` now returns the paragraph, which is the
check the audit ran and recorded as returning nothing. `npx prettier --check README.md` passes.

**State: closed.**

## Suite

- `cargo test --workspace`: green, no failures anywhere in the workspace.
- `cargo test --package sublore --bin sublore`: 7 passed, up from 4.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- No behavioural check was run for this change. The behaviour it touches is one stderr line printed
  before the window exists, and the four-launch table above measures it in the real process; the
  paint check that consumes the line needs NVIDIA hardware and a display, and its contract (the
  regex and the three captured values) is unchanged.

**Warning for whoever runs the battery next:** `cargo test` and `cargo clippy` above overwrote
`target/debug/sublore` with a plain cargo debug binary, which looks for the Vite dev server instead
of the embedded assets. Run `pnpm e2e:build` before any behavioural check (WORKFLOW 4c).

## Not fixed, and why

- The mpv side of the empty-value agreement (`player.rs:66`) and its `var(..).ok()` report
  (`player.rs:209`). Both are the same two defects in another implementer's file this round. The
  rule to follow is stated at the top of this report.
- V1 finding 6 (an unrecognised `SUBLORE_MPV_GPU_CONTEXT` value is forwarded verbatim and costs the
  user all video) is `player.rs`, not mine.
- The owner-ruled rows next to mine in the audit (`main.rs:14`, `:26`, `:27`: the breadth of the
  `/sys/module/nvidia` signal, and the armed-versus-disarmed cost measured against CLAUDE.md §7 on
  real hardware) are decisions, not defects, and are still owed by the owner. Nothing here changes
  the signal or claims a measurement.

Nothing was committed.
