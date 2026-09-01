/* global describe, it, before, document, window */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** The surface travels over IPC after layout, so its geometry lags the DOM by a frame or two. */
const TOLERANCE_PX = 2;

/** Centre of an element in physical pixels, which is what X11 pointer coordinates are. */
function centreOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, selector);
}

async function clickElement(toplevel, selector) {
  const centre = await centreOf(selector);
  if (centre === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

describe("video playback", () => {
  let toplevel = null;

  before(async () => {
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    // The toplevel is mapped before React renders into it; interacting earlier is a race.
    await waitFor(() => browser.execute(() => document.querySelector(".bar__button") !== null), {
      timeout: 30000,
      message: "the app UI to render",
    });
  });

  it("opens the sample fixture", async () => {
    const fixture = requireVideoFixture();

    // The path is chosen in the system chooser now: T1 removed every field for typing one, so
    // the route in changed and what is asserted below did not.
    await clickElement(toplevel, ".bar__button");
    const chooser = await waitForChooser("Choose a video");
    await answerChooser(chooser, fixture, "video");
    focusWindow(toplevel.id);

    // Readiness has no other signal: the empty placeholder is gone and the controls are enabled.
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".controls__button")?.disabled === false,
        ),
      { timeout: 30000, timeoutMsg: "the fixture never reached the ready state" },
    );

    const error = await browser.execute(
      () => document.querySelector(".app__error")?.textContent ?? null,
    );
    expect(error).toBe(null);
  });

  it("sizes the native video surface over the stage", async () => {
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
    expect(stage.width).toBeGreaterThan(0);
    expect(stage.height).toBeGreaterThan(0);

    // X11 geometry is physical; the DOM rect is in CSS pixels.
    const expected = {
      x: Math.round(stage.x * stage.dpr),
      y: Math.round(stage.y * stage.dpr),
      width: Math.round(stage.width * stage.dpr),
      height: Math.round(stage.height * stage.dpr),
    };

    // Direct children only: mpv draws into a window of its own inside the surface.
    const near = (a, b) => Math.abs(a - b) <= TOLERANCE_PX;
    const surface = await waitFor(
      () =>
        childWindows(toplevel.id).find(
          (child) =>
            child.width > 0 &&
            child.height > 0 &&
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

    expect(mapState(surface.id)).toBe("IsViewable");
  });
});
