# N2c / probe P3 — the two scale numbers, part one — 2026-08-30

Partial. Two of the three facts are established without running anything; the third needs the app on the owner's display and waits for the N1b measurement to free the machine.

## Measured, on the owner's session

Read from `:0` with `xrandr`, `xdpyinfo`, `xrdb` and the KDE output configuration. No process was started.

| fact                                            | value                                   |
| ----------------------------------------------- | --------------------------------------- |
| primary output                                  | DP-5, 3840x2160 at +0+0                 |
| second output                                   | DP-4, 2880x1620 at +3840+0              |
| X screen                                        | 6720x2160                               |
| `Xft.dpi`                                       | **144**                                 |
| `GDK_SCALE`, `GDK_DPI_SCALE`, `QT_SCALE_FACTOR` | all unset                               |
| KDE output scales                               | one output at **1.5**, every other at 1 |

144 is 1.5 x 96 exactly. Under rootless XWayland this is how KWin hands a fractional scale to X11 clients: the X coordinate space stays native — the 4K output really is 3840x2160 in X — and the 1.5 arrives as a font/UI DPI setting, not as a coordinate transform.

## Read from source: `window.scale_factor()` cannot carry 1.5

`tao-0.35.3/src/platform_impl/linux/window.rs:431` returns `self.scale_factor.load(..) as f64`, and the field at line 54 is an `Rc<AtomicI32>` seeded from GTK's own window scale factor. It is an integer by construction, so 1.5 is not a value it can hold. The monitor path (`monitor.rs:49`) is the same, `as f64` over an integer.

**Inference, not measurement:** with `GDK_SCALE` unset, GTK takes its integer scale factor from the XSETTINGS `Gdk/WindowScalingFactor`, which KWin leaves at 1 for fractional scales because it expresses them through `Xft.dpi` instead. So `window.scale_factor()` is expected to report **1** here. That expectation is what the remaining half of this probe checks; it is not yet a measured number.

The consequence for the code either way: `physical()` in `surface/mod.rs:51` multiplies by a factor that on this display is 1, and `logical()` — the path Linux actually uses (`surface/linux.rs:63-64`) — passes the page's numbers through untouched. Neither can produce a 1.5.

## Not yet measured: `devicePixelRatio` in the app's webview

This is the number that decides N2c's real criterion, and it needs the app running on that display. Two outcomes, agreed in advance so the result is not read backwards:

- **It reports 1.** Then the page's CSS pixel is one X pixel, no ratio is missing anywhere, and the misplaced rectangle is not a scale-conversion bug at all. The next question becomes how the rectangle is measured inside a page whose layout `Xft.dpi` has enlarged, and the fix is in the frontend's measurement, not in the surface.
- **It reports 1.5.** Then the page speaks CSS pixels while the X window needs X pixels, the Linux path is missing exactly that multiplication, and the multiplier has to come from the frontend, because `window.scale_factor()` cannot supply it. N2c's unit test then pins a conversion driven by a value the backend receives rather than one it computes.

Either way the acceptance criterion written on 2026-08-30 — a unit test of the coordinate conversion at fractional factors — is only meaningful once this number is known, which is why the backlog entry now puts this probe first.

## Method note

Everything above cost the machine nothing, which was the point: the N1b measurement was running sequentially and by owner ruling must not share the machine with parallel load. Launching the app on `:0` to read `devicePixelRatio` is allowed on the real display — launch and arguments only, no synthetic input — but it is load, so it waits.
