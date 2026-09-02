import os from "node:os";

/**
 * Where the harness stops being portable, and the one place that says so.
 *
 * The libraries beside this one drive X11 and POSIX process groups. Each of those has a Windows
 * counterpart, and none of it has ever been run on Windows, so the parts that cannot work there
 * fail here by name instead of failing later as a broken assertion — or, worse, as a call that
 * quietly does nothing and reads as a pass. BACKLOG MW.1b writes the Windows side; the `owes`
 * string at each call site is what it owes.
 */
const harnessPlatform = os.platform();

/**
 * Refuse a Linux-only backend anywhere else.
 * @param {string} seam the function a Windows implementation replaces
 * @param {string} owes what that implementation has to do
 */
export function requireLinuxBackend(seam, owes) {
  if (harnessPlatform === "linux") {
    return;
  }
  throw new Error(
    `${seam} is the Linux harness backend and this is ${harnessPlatform}. A Windows counterpart ` +
      `has to ${owes}, and BACKLOG MW.1b writes it against a machine that can run it. Nothing ` +
      `here guesses at one.`,
  );
}
