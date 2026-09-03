/* global describe, it, before, document, window */
/**
 * M2.4 W6: the waveform's bottom edge is draggable, its height outlives the session, and View turns
 * the panel off.
 *
 * The drag is a real press, move and release through X11, the way `video.spec.js` drives the seek
 * slider: a synthetic pointer event dispatched into the page would exercise React and prove nothing
 * about whether a hand can grab a four-pixel strip.
 */
import { writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, dragAt, focusWindow } from "../lib/input.js";
import { requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** Mirrors `MIN_WAVEFORM_HEIGHT` in src/App.tsx and src-tauri/src/layout.rs. */
const MINIMUM = 64;

/** The height the panel opens at before anything has been dragged. Mirrors `tools.css`. */
const DEFAULT = 128;

function rectOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return {
      x: rect.x * dpr,
      y: rect.y * dpr,
      width: rect.width * dpr,
      height: rect.height * dpr,
      cssHeight: rect.height,
      midX: (rect.x + rect.width / 2) * dpr,
      midY: (rect.y + rect.height / 2) * dpr,
    };
  }, selector);
}

async function heightOf(selector) {
  const rect = await rectOf(selector);
  return rect === null ? null : rect.cssHeight;
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/**
 * No window manager under Xvfb, so the toplevel origin is also the viewport origin.
 *
 * The destination is kept inside the window: `xdotool` refuses a pointer off the screen, and a drag
 * that asks for one fails as an error rather than as a sash that stopped where it was told to.
 */
async function dragSashBy(toplevel, dy) {
  const sash = await rectOf(".sash");
  if (sash === null) {
    throw new Error(".sash is missing from the DOM, so there is nothing to drag");
  }
  const fromX = toplevel.absX + sash.midX;
  const fromY = toplevel.absY + sash.midY;
  const toY = Math.min(
    Math.max(fromY + dy, toplevel.absY + 1),
    toplevel.absY + toplevel.height - 2,
  );
  dragAt(fromX, fromY, fromX, toY);
  // The release is what stores the height, so this reads a settled panel, never a mid-drag one.
  await browser.pause(250);
  return { fromHeight: sash.cssHeight, landedAt: Math.round(toY - toplevel.absY) };
}

async function clickElement(toplevel, selector) {
  const rect = await rectOf(selector);
  if (rect === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  clickAt(toplevel.absX + rect.midX, toplevel.absY + rect.midY);
}

async function openTheFixture(toplevel) {
  await clickElement(toplevel, ".toolbar__open-video");
  const chooser = await waitForChooser("Choose a video");
  await answerChooser(chooser, requireWaveformFixture(), "video");
  focusWindow(toplevel.id);
  await waitFor(() => present(".waveform"), {
    timeout: 30000,
    message: "the waveform panel to appear",
  });
}

async function attachToApp() {
  const toplevel = await waitFor(findToplevel, {
    timeout: 30000,
    message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
  });
  focusWindow(toplevel.id);
  await waitFor(
    () => browser.execute(() => document.querySelector(".toolbar__open-video") !== null),
    {
      timeout: 30000,
      message: "the app UI to render",
    },
  );
  return toplevel;
}

describe("the waveform sash", () => {
  let toplevel = null;

  before(async () => {
    requireWaveformFixture();
    toplevel = await attachToApp();
    await openTheFixture(toplevel);
  });

  it("makes the waveform taller and the current line shorter, and leaves the video and the grid", async () => {
    const before = {
      waveform: await heightOf(".waveform"),
      line: await heightOf(".currentline"),
      video: await heightOf(".shell__video"),
      grid: await heightOf(".shell__grid"),
    };
    expect(before.waveform).toBe(DEFAULT);

    await dragSashBy(toplevel, 40);

    const after = {
      waveform: await heightOf(".waveform"),
      line: await heightOf(".currentline"),
      video: await heightOf(".shell__video"),
      grid: await heightOf(".shell__grid"),
    };
    // Numeric matchers rather than the string trick used for booleans elsewhere: these already
    // print both sides, and what a failure here needs is the two heights.
    expect(after.waveform).toBeGreaterThan(before.waveform);
    expect(after.line).toBeLessThan(before.line);
    expect(after.video).toBe(before.video);
    expect(after.grid).toBe(before.grid);
  });

  it("stops at a floor rather than letting the panel reach zero", async () => {
    await dragSashBy(toplevel, -1000);
    const waveform = await heightOf(".waveform");
    expect(`the floor is ${MINIMUM} and the panel is ${waveform}`).toBe(
      `the floor is ${MINIMUM} and the panel is ${MINIMUM}`,
    );
    expect(await heightOf(".currentline")).toBeGreaterThan(0);
  });

  it("stops at a ceiling rather than pushing the current line out of the column", async () => {
    await dragSashBy(toplevel, 1000);
    const line = await heightOf(".currentline");
    const tools = await heightOf(".shell__tools");
    const waveform = await heightOf(".waveform");
    expect(line).toBeGreaterThan(0);
    expect(waveform).toBeLessThan(tools);
    // Back to something ordinary, so the tests after this one start from a usable stage.
    await dragSashBy(toplevel, -1000);
    await dragSashBy(toplevel, 60);
  });

  it("opens at the height the sash was left at", async () => {
    const left = await heightOf(".waveform");
    expect(left).toBeGreaterThan(MINIMUM);

    await browser.execute(() => {
      window.name = "before-the-relaunch";
    });
    await browser.reloadSession();
    toplevel = await attachToApp();
    expect(await browser.execute(() => window.name)).not.toBe("before-the-relaunch");

    await openTheFixture(toplevel);
    expect(await heightOf(".waveform")).toBe(left);
  });

  it("opens at the default height when the stored layout cannot be read", async () => {
    // Written between the two sessions, so the next launch is the one that meets it. The backend
    // says so in its log and the shell shows nothing: an unreadable layout costs one drag.
    const stored = path.join(process.env.SUBLORE_E2E_DATA_HOME, "com.sublore.app", "layout.json");
    writeFileSync(stored, '{"waveformHeight": ');

    await browser.reloadSession();
    toplevel = await attachToApp();
    await openTheFixture(toplevel);

    expect(await heightOf(".waveform")).toBe(DEFAULT);
    expect(`nothing on screen says so: ${await present(".statusbar__waveform-error")}`).toBe(
      "nothing on screen says so: false",
    );
  });

  it("turns the panel off from the View menu and gives its space back", async () => {
    const withPanel = await heightOf(".currentline");
    await clickElement(toplevel, ".menubar__title--view");
    await clickElement(toplevel, ".menubar__item--waveform-panel");
    await waitFor(async () => !(await present(".waveform")), {
      timeout: 5000,
      message: "the waveform panel to go",
    });
    expect(`the sash went with it: ${await present(".sash")}`).toBe("the sash went with it: false");
    expect(`the line took the space: ${(await heightOf(".currentline")) > withPanel}`).toBe(
      "the line took the space: true",
    );

    await clickElement(toplevel, ".menubar__title--view");
    await clickElement(toplevel, ".menubar__item--waveform-panel");
    await waitFor(() => present(".waveform"), {
      timeout: 5000,
      message: "the waveform panel to come back",
    });
    expect(await heightOf(".currentline")).toBe(withPanel);
  });
});
