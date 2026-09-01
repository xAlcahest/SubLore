/* global describe, it, before, document, window */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, dragAt, focusWindow } from "../lib/input.js";
import { requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** The surface travels over IPC after layout, so its geometry lags the DOM by a frame or two. */
const TOLERANCE_PX = 2;
/** Whole physical pixels are all an X11 pointer can be put on, and a rect's centre is a fraction. */
const POINTER_SLOP_PX = 2;
/** fixtures/video/make-sample.sh writes 30 fps, so even an exact seek lands on a frame boundary. */
const FRAME_SECONDS = 1 / 30;

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

/**
 * The playback position in seconds, read from the slider rather than the clock text, which is
 * floored to whole seconds (VideoControls.tsx). Both are written from mpv's own `time-pos`.
 */
async function position() {
  const raw = await browser.execute(
    () => document.querySelector(".controls__slider")?.value ?? null,
  );
  if (raw === null) {
    throw new Error(".controls__slider is missing: there is no playback position to read");
  }
  return Number(raw);
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

  it("seeks the video to where the slider is dragged", async () => {
    const slider = await browser.execute(() => {
      const element = document.querySelector(".controls__slider");
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
        duration: Number(element.max),
      };
    });
    if (slider === null) {
      throw new Error(".controls__slider is missing from the DOM");
    }
    expect(slider.duration).toBeGreaterThan(0);
    const label = await browser.execute(
      () => document.querySelector(".controls__button")?.textContent ?? null,
    );
    // Paused, so nothing but the drag can move the number the assertions below read.
    expect(label).toBe("Play");

    /**
     * The drop lands on the horizontal centre of the track, which is the one x where the thumb's
     * own width cancels out: a range input maps the pointer across the track minus the thumb, and
     * that mapping puts the exact midpoint of min..max under the midpoint of the rect whatever the
     * thumb measures. So the expected value is arithmetic, not a guess about WebKit's metrics, and
     * what is left to absorb is the pointer rounding and the frame the seek lands on.
     */
    const target = slider.duration / 2;
    const tolerance =
      (POINTER_SLOP_PX / (slider.width * slider.dpr)) * slider.duration + FRAME_SECONDS;
    const y = toplevel.absY + (slider.y + slider.height / 2) * slider.dpr;
    const from = toplevel.absX + (slider.x + POINTER_SLOP_PX) * slider.dpr;
    const to = toplevel.absX + (slider.x + slider.width / 2) * slider.dpr;

    dragAt(from, y, to, y);

    const dropped = await waitFor(
      async () => {
        const value = await position();
        return Math.abs(value - target) <= tolerance ? value : null;
      },
      {
        timeout: 15000,
        message:
          `the slider to report ${target.toFixed(2)}s (+/-${tolerance.toFixed(2)}s) after being ` +
          `dragged to the middle of its track`,
      },
    );

    // The reading above is still only what the app asked for: `seek` writes the target into the
    // position the moment it sends it. Playing puts mpv's own clock back in the number, so a seek
    // that never reached mpv counts up from where the file was instead of from the drop.
    const startedAt = Date.now();
    await clickElement(toplevel, ".controls__button");
    const playing = await waitFor(
      async () => {
        const value = await position();
        return value > dropped ? value : null;
      },
      {
        timeout: 15000,
        message: `playback to carry on from the ${dropped.toFixed(2)}s the drag left it at`,
      },
    );
    // Carried on from the drop rather than landing somewhere else: everything past the target is
    // the time the clip really spent playing while this waited.
    expect(playing).toBeLessThanOrEqual(target + tolerance + (Date.now() - startedAt) / 1000);

    await clickElement(toplevel, ".controls__button");
  });
});
