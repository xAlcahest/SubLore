/* global describe, it, before, document, window, getComputedStyle */
/**
 * M2.4 W5: the panel draws what the peak job produces.
 *
 * The waveform is asked what it drew, with `getImageData` on its own backing store, and never
 * photographed. Decision 26 refused a pixel instrument built on screen capture after 336 readings
 * showed it could not discriminate under Xvfb; this is the other thing. Nothing here goes through a
 * compositor, a display or a screenshot: the canvas is in this process and it answers for itself.
 *
 * The fixture is the milestone's own: six ten-second blocks alternating a full-scale 440 Hz tone
 * and digital silence, tone first, every block edge on an exact sample boundary.
 */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** The fixture's blocks, in seconds, and whether the audio in each is a tone. Tone first. */
const BLOCKS = [
  { from: 0, to: 10, tone: true },
  { from: 10, to: 20, tone: false },
  { from: 20, to: 30, tone: true },
  { from: 30, to: 40, tone: false },
  { from: 40, to: 50, tone: true },
  { from: 50, to: 60, tone: false },
];

const DURATION_S = 60;

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

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/**
 * What the app is showing, for a wait that ran out. The panel is absent until the first peak
 * arrives, so a bare timeout cannot tell a job that never started from one that failed from one
 * that is merely slow; this reads the three places that each leave a different mark.
 */
async function whatTheAppShows() {
  const state = await browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    const error = document.querySelector(".statusbar__waveform-error");
    const slider = document.querySelector(".controls__slider");
    return {
      panel: document.querySelector(".waveform") !== null,
      canvas: canvas === null ? null : { width: canvas.width, height: canvas.height },
      error: error === null ? null : error.textContent,
      at: slider === null ? null : Number(slider.value),
      duration: slider === null ? null : Number(slider.max),
    };
  });
  const panel =
    state.canvas === null
      ? `no .waveform__canvas (panel ${state.panel ? "present" : "absent"})`
      : `a ${state.canvas.width}x${state.canvas.height} canvas`;
  const video =
    state.duration === null
      ? "no transport, so no video is open"
      : `the video is open, ${state.at} of ${state.duration}s`;
  const failed = state.error === null ? "no failure is on the status bar" : `"${state.error}"`;
  return `${video}; ${panel}; ${failed}`;
}

/**
 * How tall the drawn wave is at one moment in the media, as a fraction of the canvas height, read
 * out of the canvas's own pixels.
 *
 * The column is found from the time rather than from a pixel guess: the drawing spans the whole
 * media, so second `at` is at `at / duration` of the width. A column counts as ink when it differs
 * from the background, and the answer is how far the ink reaches from the middle. Silence draws a
 * one-pixel line through the centre, so it reads near zero without reading as nothing.
 */
function inkAt(seconds, duration) {
  return browser.execute(
    (at, span) => {
      const canvas = document.querySelector(".waveform__canvas");
      if (canvas === null) {
        return null;
      }
      const context = canvas.getContext("2d");
      if (context === null) {
        return null;
      }
      const x = Math.min(canvas.width - 1, Math.floor((at / span) * canvas.width));
      const column = context.getImageData(x, 0, 1, canvas.height).data;
      // The background comes from the page, never from a pixel: sampling (0,0) reads the wave when
      // the media opens on a loud passage, and then every background pixel counts as painted.
      const painted_ = getComputedStyle(document.querySelector(".waveform")).backgroundColor;
      const background = (painted_.match(/\d+/g) ?? ["0", "0", "0"]).map(Number);
      const middle = canvas.height / 2;
      let reach = 0;
      let painted = 0;
      for (let y = 0; y < canvas.height; y += 1) {
        const i = y * 4;
        const differs =
          Math.abs(column[i] - background[0]) +
            Math.abs(column[i + 1] - background[1]) +
            Math.abs(column[i + 2] - background[2]) >
          24;
        if (differs) {
          painted += 1;
          reach = Math.max(reach, Math.abs(y + 0.5 - middle));
        }
      }
      return { reach: reach / middle, painted, height: canvas.height };
    },
    seconds,
    duration,
  );
}

async function openTheFixture(toplevel) {
  await clickElement(toplevel, ".toolbar__video-open");
  const chooser = await waitForChooser("Choose a video");
  await answerChooser(chooser, requireWaveformFixture(), "video");
  focusWindow(toplevel.id);
}

/** The playback position in seconds, from the slider, as `video.spec.js` reads it. */
async function position() {
  const raw = await browser.execute(
    () => document.querySelector(".controls__slider")?.value ?? null,
  );
  if (raw === null) {
    throw new Error(".controls__slider is missing: there is no playback position to read");
  }
  return Number(raw);
}

describe("the waveform draws what the job produces", () => {
  let toplevel = null;

  before(async () => {
    requireWaveformFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__video-open") !== null),
      {
        timeout: 30000,
        message: "the app UI to render",
      },
    );
  });

  it("has no waveform panel before a video is opened", async () => {
    // The panel is absent rather than empty: a panel with no provider takes no space, and an empty
    // one would be the placeholder shell-layout.md refuses.
    expect(await present(".waveform")).toBe(false);
  });

  it("shows the panel while the peaks are still arriving, not when the job ends", async () => {
    await openTheFixture(toplevel);

    // The panel is up before the whole 60 s has been peaked. Asserted against the job's own report
    // rather than a stopwatch: `audio://done` has not been seen while this holds.
    let drawnEarly = null;
    try {
      drawnEarly = await waitFor(
        async () => {
          if (!(await present(".waveform"))) {
            return null;
          }
          const ink = await inkAt(1, DURATION_S);
          return ink !== null && ink.painted > 0 ? ink : null;
        },
        { timeout: 30000, message: "the waveform panel to appear and hold ink" },
      );
    } catch (error) {
      throw new Error(`${error.message}\nwhen the wait ran out: ${await whatTheAppShows()}.`);
    }
    expect(drawnEarly.painted).toBeGreaterThan(0);
  });

  it("reads full where the fixture is a tone and flat where it is silent", async () => {
    // The playhead is a full-height line and this reading counts every painted pixel, so where it
    // sits matters. It is at the start here, five seconds from the nearest block centre, and that
    // is asserted rather than assumed: a change that made a file start playing on open would move
    // the head onto a block and this check would start reading it as a tone.
    expect(await position()).toBeLessThan(1);

    // Every block centre, once the whole file has been peaked. This is the milestone's own
    // sentence, and it is answered by the canvas rather than by a screenshot.
    const readings = await waitFor(
      async () => {
        const all = [];
        for (const block of BLOCKS) {
          const ink = await inkAt((block.from + block.to) / 2, DURATION_S);
          if (ink === null) {
            return null;
          }
          all.push({ ...block, ...ink });
        }
        return all.every((r) => (r.tone ? r.reach > 0.8 : r.painted > 0)) ? all : null;
      },
      {
        timeout: 60000,
        message: "every block centre to be drawn",
      },
    ).catch(async (error) => {
      throw new Error(`${error.message}\nwhen the wait ran out: ${await whatTheAppShows()}.`);
    });

    for (const reading of readings) {
      const where = `${reading.from}-${reading.to}s`;
      if (reading.tone) {
        // The tone is full scale; the smallest bucket over this fixture is 98.2% of it, so a wave
        // that reaches less than four fifths of the half-height is not a tone.
        expect(`${where}: ${reading.reach > 0.8}`).toBe(`${where}: true`);
      } else {
        // Silence is a line through the middle: painted, so it cannot be confused with peaks that
        // have not arrived, and no taller than a few pixels.
        expect(`${where} painted: ${reading.painted > 0}`).toBe(`${where} painted: true`);
        expect(`${where} flat: ${reading.reach < 0.15}`).toBe(`${where} flat: true`);
      }
    }
  });

  it("leaves the video surface on its stage rectangle after the panel arrives", async () => {
    // M0.2: a sibling appearing in the tools column must not translate the video panel, because a
    // ResizeObserver cannot see a translation and the native surface would be left behind.
    const stage = await browser.execute(() => {
      const element = document.querySelector(".stage__surface");
      if (element === null) {
        return null;
      }
      const rect = element.getBoundingClientRect();
      const dpr = window.devicePixelRatio;
      return {
        width: Math.round(rect.width * dpr),
        height: Math.round(rect.height * dpr),
      };
    });
    expect(stage).not.toBe(null);
    expect(stage.width).toBeGreaterThan(0);
    expect(stage.height).toBeGreaterThan(0);

    const scrolled = await browser.execute(() => {
      const panel = document.querySelector(".shell__video");
      return panel === null ? null : panel.scrollHeight > panel.clientHeight;
    });
    expect(scrolled).toBe(false);
  });
});
