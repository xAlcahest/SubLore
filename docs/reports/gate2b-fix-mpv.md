# Gate 2, wave 4b — the mpv `gpu-context` decision

**File touched:** `src-tauri/src/video/player.rs`, and nothing else.
**Findings addressed:** V1 finding 2 (the pin narrowed to Wayland sessions only) and V1 finding 6
(the hatch turns a typo into no video at all).
**Platform:** every measurement below was taken on Linux, Fedora, mpv 0.41.0 / libmpv 2.5.0,
libwayland-client 1.25, on the owner's machine. Nothing here is a Windows claim; the code is inside
`#[cfg(target_os = "linux")]` and Windows keeps taking no `gpu-context` at all.

---

## 1. The pin is unconditional again, and this time it was measured against Sublore's own surface

### What changed

`gpu_context_for` lost its `wayland_display` argument. It now pins `x11egl` for every X11 `wid`,
which is what `eca9806` did, and the hatch `SUBLORE_MPV_GPU_CONTEXT` is the way off it (a name
forces that name, an empty value hands the choice back to mpv). The docstring no longer carries the
python-xlib measurement that did not license it.

### Why: what mpv's `auto` actually does here

Six runs of the `mpv` binary, one Xvfb display each, `--vo=gpu --gpu-context=auto`, reading which
context mpv settles on:

| environment                                                  | context chosen by `auto`                              | usable for a `wid`         |
| ------------------------------------------------------------ | ----------------------------------------------------- | -------------------------- |
| `WAYLAND_DISPLAY=wayland-0`                                  | `waylandvk`                                           | no, this is the N2b defect |
| `WAYLAND_DISPLAY` unset, socket present in `XDG_RUNTIME_DIR` | `x11vk`                                               | yes                        |
| `WAYLAND_DISPLAY` unset, Vulkan disabled                     | `x11egl`                                              | yes                        |
| `WAYLAND_DISPLAY` unset, Vulkan and EGL disabled             | none: "Failed initializing any suitable GPU context!" | **no video at all**        |

The last row is the one that decided this. The narrowing's stated reason was that pinning EGL would
"remove the GLX fallback a machine without EGL depends on". On this stack that fallback does not
work: inside `auto`, after the EGL attempt, the GLX context picks an FB config and a visual and then
still fails, and `auto` walks off the end of its list. Forcing `--gpu-context=x11` in the same
environment plays the file. So `auto` does not save an EGL-less machine, while the hatch does, and
the pin costs that machine nothing it had.

### The defect the pin exists for, reproduced against Sublore's own surface

This is the measurement V1 asked for, and it is not a probe window: it is `pnpm e2e:wayland`, which
launches the app itself on the owner's real Wayland session and looks for mpv's window inside the
GDK native child created by `video/surface/linux.rs`.

| run                                   | environment                               | result                                                                                        |
| ------------------------------------- | ----------------------------------------- | --------------------------------------------------------------------------------------------- |
| pinned (shipping default)             | real session, `WAYLAND_DISPLAY=wayland-0` | passed 4/4, `gpu-context x11egl`, mpv's window inside the surface                             |
| unpinned (`SUBLORE_MPV_GPU_CONTEXT=`) | same session, same binary                 | **failed**: "the surface has no children: mpv took the Wayland display and drew past the wid" |

Same binary, same machine, one environment variable apart. That is the discrimination experiment for
the pin, taken through the product rather than through python-xlib.

### The branch that used to be unpinned, now pinned, is covered

Under Xvfb with `WAYLAND_DISPLAY` scrubbed by `appEnv`, the full wdio suite is green with the pin:
8 spec files, 33 tests, run on display `:152`. `video-surface.spec.js`'s `before` hook is the check
that matters: it waits for mpv's own child window inside the surface and throws if it never appears
(`e2e/specs/video-surface.spec.js:138-155`). That hook runs in CI on every push, so a pin that stops
attaching on the X11 branch turns the Linux job red. A single launch on display `:161` through the
same X11 path logs `video: gpu-context x11egl (SUBLORE_MPV_GPU_CONTEXT=unset, WAYLAND_DISPLAY=unset)`
and reports `surface IsViewable, mpv children 1`.

To be precise about what that green does and does not prove: on this machine both the pin and `auto`
attach on the X11 branch, so the wdio run shows the pin does not break X11, not that X11 needed it.
The machines where the two differ are the ones in the table above, measured at the mpv level.

### A test was replaced, and this names it (WORKFLOW.md §4)

`without_a_wayland_display_mpv_keeps_its_own_probing` is gone. It asserted the behaviour this fix
removes, and its docstring carried the python-xlib measurement V1 flagged. Its replacement,
`every_x11_wid_gets_the_pinned_context`, asserts the opposite and fails when the pin is removed
(shown below). No assertion was weakened: the count of decision tests is unchanged and three of them
are new.

---

## 2. A mistyped hatch no longer costs the user all video

### What changed

`set_gpu_context` sets the requested context and, when mpv refuses the value, falls back to the pin
and returns `FellBack` so `Player::new` can warn. A pin mpv itself refuses is propagated rather than
retried, so a genuinely broken mpv still surfaces its error instead of looping.

### What proves it, live

Same binary, same session, one environment variable, before and after the fix:

| build                                                | `SUBLORE_MPV_GPU_CONTEXT=x11eg1` | result                                                                                                                  |
| ---------------------------------------------------- | -------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| without the fallback (rebuilt deliberately to check) | forwarded verbatim               | `video setup failed: CommandFailed: mpv error -7 (mpv initialisation)`, the surface never gets mpv, `e2e:wayland` fails |
| with the fallback (shipping)                         | refused, pin used                | `WARN video: mpv refused gpu-context x11eg1 from SUBLORE_MPV_GPU_CONTEXT, using x11egl`, `e2e:wayland` passes 4/4       |

The "without" row was produced by editing the fix out, running `pnpm e2e:build` and checking its
exit status explicitly (`BUILD_EXIT=0`, "Built application" present) before the run, then restoring
and rebuilding. That is the WORKFLOW §4c rule about experiments that never ran.

---

## 3. Tests, and that they can fail

`cargo test` for the crate: 133 tests, all passing, and `cargo clippy --all-targets` clean. The six
tests in `video::player::tests` are the ones this delivery owns:

- `every_x11_wid_gets_the_pinned_context`
- `the_hatch_forces_a_context_where_the_pin_is_wrong`
- `an_empty_hatch_hands_the_choice_back_to_mpv`
- `a_context_mpv_accepts_is_the_one_that_reaches_it`
- `a_context_mpv_refuses_falls_back_to_the_pin`
- `a_pin_mpv_refuses_reaches_the_caller`

The fake `set_option` in the last three refuses every name outside mpv 0.41's own
`--gpu-context=help` list, which is what lets the fallback test fail rather than assert on a
constant.

Two removal experiments, run rather than argued:

- Change the decision back to `None => None`: `every_x11_wid_gets_the_pinned_context` FAILED, the
  other five passed.
- Replace the fallback with a plain forward: `a_context_mpv_refuses_falls_back_to_the_pin` FAILED,
  the other five passed.

Both were restored and the six passed again before anything else was run.

---

## 4. What is not proved, stated plainly

- **The EGL-less machine was simulated at the mpv level only.** Disabling the EGL vendor library
  takes Sublore's own webview down with it: on displays `:162`, `:163` and `:164` the app never
  produced its surface at all, with the pin, with `auto` and with the GLX hatch alike. So the claim
  "with EGL missing, `auto` gives up and the hatch works" is a measurement of mpv, not of Sublore.
  What it does establish is that the fallback the narrowing was protecting is not there.
- **I could not reproduce a Wayland takeover with `WAYLAND_DISPLAY` unset.** With the socket present
  in `XDG_RUNTIME_DIR` mpv's Wayland contexts refuse to connect, and handing mpv a live connection
  through `WAYLAND_SOCKET` did not change that (the fd is single use and something earlier in mpv's
  startup consumes it; `wayland-info` on the same fd connected fine, so the probe itself worked). So
  the narrowed condition was not shown to be wrong on this stack today. It was shown to be a proxy
  for an mpv-internal decision that reads two variables where the app read one, resting on mpv's
  probe order staying as it is, and to be buying a fallback that does not exist. That is why it is
  gone rather than widened.
- **The pixels.** `e2e:wayland` asserts mpv's child window, not saturation, by its own design; the
  pixel evidence for `x11egl` on real hardware is `docs/reports/n2b-collaudo-reale.md` and was not
  re-taken here.
- **Windows.** Untouched and unrun. The whole decision is Linux-only by `cfg`.

---

## 5. State

| finding                                     | state                                                                                                  |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| V1 #2, the pin narrowed to Wayland sessions | **fixed**, unconditional again, discriminated live on the real session and green under Xvfb            |
| V1 #6, a hatch typo costs all video         | **fixed**, refused values fall back to the pin with a warning, proved by a before/after pair of builds |

The binary at `target/debug/sublore` is a `pnpm e2e:build` of exactly this code: the last thing
run after it was the behavioural battery, never `cargo test` or `clippy` (WORKFLOW.md §4c).

Not committed, per the brief. `BACKLOG.md:94` and `docs/design/x11-vs-render-api.md:23` describe the
pin as unconditional; they were left stale by the previous round and are correct again as of this
change, so no documentation edit was needed and none was made (those files are not mine).
