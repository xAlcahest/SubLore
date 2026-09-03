/* global document, window */
/**
 * Where the native video surface is, asserted through the X11 window tree rather than the picture.
 *
 * M0.2 places the surface from the stage's DOM coordinates and recomputes it on every resize, so
 * every check that resizes anything above or beside the video is a check on this. Written for
 * `shell.spec.js` and shared with `dividers.spec.js`, which drags the two edges that resize it.
 */
import { browser, expect } from "@wdio/globals";

import { waitFor } from "./proc.js";
import { childWindows, mapState, rootTree } from "./x11.js";

/** The surface travels over IPC after layout, so its geometry lags the DOM by a frame or two. */
export const TOLERANCE_PX = 2;

/** The rectangle the native surface should be sitting on, in the physical pixels X reports. */
export async function surfaceRect() {
  const stage = await browser.execute(() => {
    const element = document.querySelector(".stage__surface");
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    return {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      dpr: window.devicePixelRatio,
    };
  });
  if (stage === null) {
    throw new Error(".stage__surface is missing from the DOM");
  }
  return {
    x: Math.round(stage.x * stage.dpr),
    y: Math.round(stage.y * stage.dpr),
    width: Math.round(stage.width * stage.dpr),
    height: Math.round(stage.height * stage.dpr),
  };
}

/** Wait for a viewable X11 child of the app window sitting on the rectangle the stage reports. */
export async function waitForSurfaceOnStage(toplevel) {
  const expected = await surfaceRect();
  expect(expected.width).toBeGreaterThan(0);
  expect(expected.height).toBeGreaterThan(0);
  const near = (a, b) => Math.abs(a - b) <= TOLERANCE_PX;
  return waitFor(
    () =>
      childWindows(toplevel.id).find(
        (child) =>
          near(child.relX, expected.x) &&
          near(child.relY, expected.y) &&
          near(child.width, expected.width) &&
          near(child.height, expected.height) &&
          mapState(child.id) === "IsViewable",
      ) ?? null,
    {
      timeout: 15000,
      message:
        `a viewable direct child of ${toplevel.id} at ${expected.width}x${expected.height}+` +
        `${expected.x}+${expected.y} (+/-${TOLERANCE_PX}px).\n${rootTree()}`,
    },
  );
}
