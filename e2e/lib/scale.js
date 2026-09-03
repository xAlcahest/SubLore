/* global document, window */
/**
 * The size the interface is drawn at, read off the rendered page (S1).
 *
 * The bounds a spec mirrors from `src/App.tsx` were all measured at one size, and `App.tsx` now
 * takes each of them against the interface size before it uses it. A spec that keeps the bare
 * number is asserting a floor the app no longer has, so it takes the same multiplication from here.
 */
import { browser } from "@wdio/globals";

/** The browser's own default root size, which the `html` rule in src/styles/shell.css multiplies. */
const ROOT_FONT_PX = 16;

/** The multiplier the root font size is drawn at now: 1 at 100 per cent, 1.1 at 110, and so on. */
export async function interfaceScale() {
  const size = await browser.execute(
    () => window.getComputedStyle(document.documentElement).fontSize,
  );
  const px = Number.parseFloat(size);
  if (!Number.isFinite(px) || px <= 0) {
    throw new Error(
      `the root font size read back as ${JSON.stringify(size)}, so there is no interface size to ` +
        `take a bound against.`,
    );
  }
  return px / ROOT_FONT_PX;
}
