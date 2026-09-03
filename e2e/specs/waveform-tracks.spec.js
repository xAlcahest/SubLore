/* global describe, it, before, console, document, window, getComputedStyle, performance */
/**
 * M2.4 W8: the Audio menu lists the media's tracks, marks the one being drawn, and switching draws
 * the other one.
 *
 * The fixture is the milestone's: two unbroken tones, the first at full scale and the second at a
 * quarter of it, so which track is drawn is a question the canvas answers on its own.
 */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import {
  requireTracksFixture,
  requireWaveformFixture,
  tracksFixture,
  waveformFixture,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { ffmpegProcessesFor, waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** W8: a switch back to a track already peaked is a cache read, not a child process. */
const CACHED_SWITCH_MS = 200;

/** How far the drawing reaches from the middle, as a fraction of the half-height. */
function reach() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    if (canvas === null) {
      return null;
    }
    const context = canvas.getContext("2d");
    const panel = getComputedStyle(document.querySelector(".waveform")).backgroundColor;
    const background = (panel.match(/\d+/g) ?? ["0", "0", "0"]).map(Number);
    const middle = canvas.height / 2;
    let highest = 0;
    // Several columns, so one that happens to sit on a zero crossing does not answer for the file.
    for (const x of [10, 30, 50, 70, 90]) {
      const column = context.getImageData(Math.min(x, canvas.width - 1), 0, 1, canvas.height).data;
      for (let y = 0; y < canvas.height; y += 1) {
        const i = y * 4;
        const differs =
          Math.abs(column[i] - background[0]) +
            Math.abs(column[i + 1] - background[1]) +
            Math.abs(column[i + 2] - background[2]) >
          24;
        if (differs) {
          highest = Math.max(highest, Math.abs(y + 0.5 - middle) / middle);
        }
      }
    }
    return highest;
  });
}

function rectOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: rect.x * dpr, y: rect.y * dpr, width: rect.width * dpr, height: rect.height * dpr };
  }, selector);
}

async function clickElement(toplevel, selector) {
  const rect = await rectOf(selector);
  if (rect === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  clickAt(toplevel.absX + rect.x + rect.width / 2, toplevel.absY + rect.y + rect.height / 2);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** Every item of the Audio menu, with whether it is marked and whether it can be chosen. */
async function audioItems(toplevel) {
  await clickElement(toplevel, ".menubar__title--audio");
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".menubar__menu [role='menuitemcheckbox']")).map(
      (node) => ({
        label: node.querySelector(".menubar__label")?.textContent ?? "",
        checked: node.getAttribute("aria-checked") === "true",
        enabled: !node.disabled,
        id: node.id,
      }),
    ),
  );
}

async function closeMenu() {
  await browser.execute(() => document.querySelector(".menubar__title--audio")?.click());
}

async function chooseTrack(toplevel, id) {
  await clickElement(toplevel, `.menubar__item--audio-track-${id}`);
}

async function openVideo(toplevel, fixture) {
  await clickElement(toplevel, ".toolbar__open-video");
  const chooser = await waitForChooser("Choose a video");
  await answerChooser(chooser, fixture, "video");
  focusWindow(toplevel.id);
}

describe("the Audio menu and the track that is drawn", () => {
  let toplevel = null;

  before(async () => {
    requireTracksFixture();
    requireWaveformFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__open-video") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
    await openVideo(toplevel, tracksFixture);
    await waitFor(() => present(".waveform"), {
      timeout: 30000,
      message: "the waveform panel for the two-track fixture",
    });
  });

  it("lists both tracks and marks the one being drawn", async () => {
    const items = await audioItems(toplevel);
    await closeMenu();
    expect(items).toHaveLength(2);
    expect(items.map((item) => item.label)).toEqual(["Japanese original", "English dub"]);
    expect(items.map((item) => item.checked)).toEqual([true, false]);
  });

  it("draws the quarter-scale track after switching to it, and the full one on the way back", async () => {
    const full = await reach();
    expect(full).toBeGreaterThan(0.8);

    await audioItems(toplevel);
    await chooseTrack(toplevel, 2);
    const quartered = await waitFor(
      async () => {
        const now = await reach();
        return now !== null && now < 0.5 ? now : null;
      },
      { timeout: 30000, message: `the drawing to drop to the quarter-scale track (was ${full})` },
    );
    console.log(`W8 amplitude: full ${full.toFixed(3)}, second track ${quartered.toFixed(3)}`);
    expect(quartered).toBeLessThan(0.5);
    expect(quartered).toBeGreaterThan(0.05);

    const items = await audioItems(toplevel);
    await closeMenu();
    expect(items.map((item) => item.checked)).toEqual([false, true]);
  });

  it("switches back to a peaked track from the cache, inside the budget and with no child", async () => {
    await audioItems(toplevel);
    // Both timestamps are taken inside the page, which is what the criterion asks for and what the
    // number needs: reading the canvas over the WebDriver bridge costs tens of milliseconds a poll,
    // and a stopwatch held out here measures the bridge as much as the app.
    await browser.execute(() => {
      const item = document.querySelector(".menubar__item--audio-track-1");
      window.__subloreSwitch = { at: null, drawn: null };
      item.addEventListener("mousedown", () => {
        window.__subloreSwitch.at = performance.now();
      });
      const watch = () => {
        // Looked up every frame: a new job empties the peaks, the panel unmounts while it has
        // nothing to draw, and a canvas held from before the switch is a detached element that
        // never changes again.
        const canvas = document.querySelector(".waveform__canvas");
        if (
          canvas !== null &&
          window.__subloreSwitch.at !== null &&
          window.__subloreSwitch.drawn === null
        ) {
          const middle = canvas.height / 2;
          const column = canvas.getContext("2d").getImageData(20, 0, 1, canvas.height).data;
          let highest = 0;
          for (let y = 0; y < canvas.height; y += 1) {
            if (
              column[y * 4 + 3] > 0 &&
              column[y * 4] + column[y * 4 + 1] + column[y * 4 + 2] > 60
            ) {
              highest = Math.max(highest, Math.abs(y + 0.5 - middle) / middle);
            }
          }
          if (highest > 0.8) {
            window.__subloreSwitch.drawn = performance.now();
            return;
          }
        }
        window.requestAnimationFrame(watch);
      };
      window.requestAnimationFrame(watch);
    });

    await chooseTrack(toplevel, 1);

    const took = await waitFor(
      () =>
        browser.execute(() => {
          const mark = window.__subloreSwitch;
          return mark.at !== null && mark.drawn !== null ? mark.drawn - mark.at : null;
        }),
      { timeout: 30000, message: "the full-scale track to be drawn again" },
    ).catch(async (error) => {
      const mark = await browser.execute(() => window.__subloreSwitch);
      throw new Error(`${error.message}\nthe probe recorded ${JSON.stringify(mark)}`);
    });
    console.log(`W8 cached switch: ${Math.round(took)} ms, budget ${CACHED_SWITCH_MS}`);
    expect(took).toBeLessThan(CACHED_SWITCH_MS);
    expect(ffmpegProcessesFor("waveform-tracks")).toEqual([]);
  });

  it("leaves no ffmpeg behind when a switch lands on top of another", async () => {
    await audioItems(toplevel);
    await chooseTrack(toplevel, 2);
    await audioItems(toplevel);
    await chooseTrack(toplevel, 1);
    await waitFor(
      async () => {
        const now = await reach();
        return now !== null && now > 0.8 ? true : null;
      },
      { timeout: 30000, message: "the drawing to settle on the track chosen last" },
    );
    expect(ffmpegProcessesFor("waveform-tracks")).toEqual([]);
  });

  it("lists a single-track file's one track and offers no switch it cannot make", async () => {
    await openVideo(toplevel, waveformFixture);
    await waitFor(() => present(".waveform"), {
      timeout: 30000,
      message: "the waveform panel for the single-track fixture",
    });

    const items = await audioItems(toplevel);
    await closeMenu();
    expect(items).toHaveLength(1);
    expect(items[0].checked).toBe(true);
    expect(items[0].enabled).toBe(false);
  });
});
