import process from "node:process";

/**
 * The environment the app is launched with, for every harness that spawns it.
 *
 * With `WAYLAND_DISPLAY` set, libmpv does not attach to the X11 surface it was handed: the surface
 * reports `IsViewable` with zero children, the stage keeps showing its placeholder, and every pixel
 * assertion measures the webview underneath while the transport happily reports playback. An N2
 * probe lost two runs to this before a screenshot gave it away.
 *
 * The first diagnosis blamed GTK and was wrong: `main.rs` already forces `GDK_BACKEND=x11` before
 * `gtk_init`, so GTK never had a choice to make. The component ignoring the `wid` is libmpv, which
 * is not pinned to an X11 output, and that is a product defect on the primary platform — the
 * owner's own session is Wayland. **BACKLOG N2b fixes it in the product.** What is left here is
 * determinism for the harness, not a cure: clearing the variable makes every run start from the
 * same place instead of from whatever the developer's shell exported.
 */
export function appEnv(overrides = {}) {
  const env = {
    ...process.env,
    GDK_BACKEND: "x11",
    // Disarmed here because the workarounds key on the driver being loaded, which is true on a
    // developer machine even under Xvfb, where llvmpipe renders and input reaches React late
    // enough to lose races. So every caller of `appEnv` tests
    // a configuration no user gets; the armed one is checked by `pnpm e2e:webview`.
    SUBLORE_WEBKIT_WORKAROUNDS: "0",
    ...overrides,
  };
  delete env.WAYLAND_DISPLAY;
  return env;
}
