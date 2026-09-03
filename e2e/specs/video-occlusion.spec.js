/* global describe, it, before, afterEach, document, window */
/**
 * T8, decision 1: the picture gets out of the way while an HTML layer is open over it.
 *
 * The surface is an X11 child of the toplevel, so it stacks above the webview by construction and
 * a menu drawn where the video is would be behind it. What is asserted here is the same signal
 * `video-surface.spec.js` uses — the X11 map state, with mpv's own child window as the second half
 * of it — and deliberately not the pixels: under Xvfb with llvmpipe the frame was measured
 * appearing 2 times in 10 while mpv was attached 10 times out of 10, so a pixel assertion would be
 * intermittent for a reason that has nothing to do with the code. That file's header and
 * e2e/README.md carry the measurement.
 *
 * The premise the criterion states — that the File dropdown lands over the video rectangle — is
 * read off the two elements rather than assumed. A layout that moved them apart would leave this
 * file asserting a hide over nothing, and it says so instead.
 */
import { setTimeout as sleep } from "node:timers/promises";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey } from "../lib/input.js";
import { requireVideoFixture, videoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** The surface travels over IPC after layout, so its geometry lags the DOM by a frame or two. */
const TOLERANCE_PX = 2;

/** How long the frame is watched for across a layer swap, and how often. See the third test. */
const SWAP_SAMPLES = 12;
const SWAP_INTERVAL_MS = 100;

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

/** The title of the open dropdown, or null when none is open. */
function openDropdown() {
  return browser.execute(
    () => document.querySelector(".menubar__menu")?.getAttribute("aria-label") ?? null,
  );
}

/** The command the menu cursor is on, by id, or null when no item carries it. */
function cursorCommand() {
  return browser.execute(
    () => document.querySelector(".menubar__item--cursor")?.id.replace("menuitem-", "") ?? null,
  );
}

/** The stage rectangle in the physical pixels X11 works in, straight from the DOM. */
async function stageRect() {
  const measured = await browser.execute(() => {
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
  if (measured === null) {
    throw new Error(".stage__surface is missing from the DOM");
  }
  return {
    x: Math.round(measured.x * measured.dpr),
    y: Math.round(measured.y * measured.dpr),
    width: Math.round(measured.width * measured.dpr),
    height: Math.round(measured.height * measured.dpr),
  };
}

/** Resize the stage the way a sash or a window resize would, and prove the DOM took it. */
async function setStageHeight(height) {
  const applied = await browser.execute((css) => {
    const element = document.querySelector(".stage__surface");
    if (element === null) {
      throw new Error(".stage__surface is missing from the DOM");
    }
    element.style.height = css;
    return element.getBoundingClientRect().height;
  }, height);
  return applied;
}

/** Whether the open dropdown really covers part of the stage, which is what the criterion says. */
function dropdownOverStage() {
  return browser.execute(() => {
    const menu = document.querySelector(".menubar__menu");
    const stage = document.querySelector(".stage__surface");
    if (menu === null || stage === null) {
      return null;
    }
    const a = menu.getBoundingClientRect();
    const b = stage.getBoundingClientRect();
    return {
      menu: { x: a.x, y: a.y, width: a.width, height: a.height },
      stage: { x: b.x, y: b.y, width: b.width, height: b.height },
      overlaps: a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top,
    };
  });
}

/**
 * The one surface, re-read every time: its geometry moves with the layout. More than one large
 * child means a leak, and saying so here keeps the finding at its cause.
 */
function surfaceWindow(toplevel) {
  const large = childWindows(toplevel.id).filter((child) => child.width > 50 && child.height > 50);
  if (large.length > 1) {
    throw new Error(
      `expected one native surface, found ${large.length} (${large.map((w) => w.id).join(", ")})` +
        `\n${rootTree()}`,
    );
  }
  return large[0] ?? null;
}

function currentSurface(toplevel) {
  const found = surfaceWindow(toplevel);
  if (found === null) {
    throw new Error(`the native surface is gone.\n${rootTree()}`);
  }
  return found;
}

function waitForHidden(toplevel, message) {
  return waitFor(() => (mapState(currentSurface(toplevel).id) === "IsUnMapped" ? true : null), {
    timeout: 15000,
    message: `${message}\n${rootTree()}`,
  });
}

/** Back on screen means mapped with mpv still inside it, never the map state alone. */
function waitForFrameBack(toplevel, message) {
  return waitFor(
    () => {
      const surface = currentSurface(toplevel);
      return mapState(surface.id) === "IsViewable" && childWindows(surface.id).length > 0
        ? true
        : null;
    },
    { timeout: 15000, message: `${message}\n${rootTree()}` },
  );
}

/**
 * The playback position in seconds, read from the slider rather than the clock text: the text is
 * floored to whole seconds (VideoControls.tsx), and both are written from mpv's own `time-pos`.
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

/** More steps than the bar can have titles, so a walk that never arrives still ends. */
const TITLE_WALK_LIMIT = 12;

describe("the picture gets out of the way for an HTML layer", () => {
  let toplevel = null;

  before(async () => {
    requireVideoFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__open-video") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );

    // Opened through the chooser, the way a person does.
    await clickElement(toplevel, ".toolbar__open-video");
    const chooser = await waitForChooser("Choose a video");
    await answerChooser(chooser, videoFixture, "video");
    // The chooser had the keyboard, and every gesture below needs the app window to have it.
    focusWindow(toplevel.id);

    // mpv attaching is the honest signal, not the map state: a surface with no mpv child reports
    // IsViewable while showing the webview underneath.
    await waitForFrameBack(toplevel, "the surface to come up with mpv attached inside it");
    await waitFor(
      () => browser.execute(() => document.querySelector(".controls__button")?.disabled === false),
      { timeout: 15000, message: "the transport button to become enabled" },
    );

    // "with a video playing" is the criterion's own precondition, so it is proved here rather than
    // assumed: a click that missed would leave every test below measuring a paused video.
    await clickElement(toplevel, ".controls__button");
    const started = await position();
    await waitFor(async () => ((await position()) > started ? true : null), {
      timeout: 15000,
      message: `the playback position to advance after Play (still ${started})`,
    });
  });

  afterEach(async () => {
    // A test that fails with a layer open would leave every later one measuring a hidden surface
    // and blaming the wrong thing. Escape closes whichever layer is up.
    pressKey("Escape");
    await setStageHeight("");
  });

  it("hides the surface for a File menu over the video and brings it back on close", async () => {
    await clickElement(toplevel, ".menubar__title--file");
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the File dropdown to open",
    });

    // The criterion says "over the video rectangle", so that is read rather than assumed.
    const geometry = await dropdownOverStage();
    if (geometry === null) {
      throw new Error("the dropdown or the stage is missing from the DOM");
    }
    if (!geometry.overlaps) {
      throw new Error(
        `the File dropdown does not land over the video rectangle, so the hide below would be ` +
          `asserted over nothing: menu ${JSON.stringify(geometry.menu)}, stage ` +
          `${JSON.stringify(geometry.stage)}.`,
      );
    }

    await waitForHidden(toplevel, "the surface to hide while the File dropdown is open");
    // The dropdown is still the open one: what hid the video did not dismiss the menu with it.
    expect(await openDropdown()).toBe("File");

    pressKey("Escape");
    await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
      timeout: 15000,
      message: "the File dropdown to close",
    });
    await waitForFrameBack(toplevel, "the frame to come back when the dropdown closes");

    // Nothing was restarted to get it back: the clock is still running afterwards.
    const back = await position();
    await waitFor(async () => ((await position()) > back ? true : null), {
      timeout: 15000,
      message: `playback to continue after the frame came back (stuck at ${back})`,
    });
  });

  it("keeps the surface hidden across the swap from the menu to the dialog it opens", async () => {
    pressKey("alt");
    await waitFor(async () => ((await openDropdown()) === "File" ? true : null), {
      timeout: 15000,
      message: "the File dropdown to open on Alt",
    });
    // Walked to the end rather than counted: decision 24 A2 gives the bar a title only when
    // something is behind it, so how many titles there are depends on what is open, and Audio comes
    // and goes with the media. Help is the last of them, which is what this needs.
    for (let step = 0; step < TITLE_WALK_LIMIT; step += 1) {
      if ((await openDropdown()) === "Help") {
        break;
      }
      pressKey("Right");
      await browser.pause(150);
    }
    await waitFor(async () => ((await openDropdown()) === "Help" ? true : null), {
      timeout: 15000,
      message: `the Help dropdown to be the open one after walking right (saw ${await openDropdown()})`,
    });
    await waitFor(async () => ((await cursorCommand()) === "about" ? true : null), {
      timeout: 15000,
      message: "the menu cursor to sit on About",
    });
    await waitForHidden(toplevel, "the surface to hide while a dropdown is open");

    // Enter closes the dropdown and opens the dialog in one update, so the frame should never come
    // back in between. A poll cannot prove a negative over an interval: what is claimed here is
    // that every sample across it read IsUnMapped, at the interval named above.
    pressKey("Return");
    await waitFor(() => present(".about"), {
      timeout: 15000,
      message: "the About dialog to open",
    });
    const samples = [];
    for (let sample = 0; sample < SWAP_SAMPLES; sample += 1) {
      samples.push(mapState(currentSurface(toplevel).id));
      await sleep(SWAP_INTERVAL_MS);
    }
    expect(samples.filter((state) => state !== "IsUnMapped")).toEqual([]);
    expect(await openDropdown()).toBe(null);

    pressKey("Escape");
    await waitFor(async () => ((await present(".about")) === false ? true : null), {
      timeout: 15000,
      message: "the About dialog to close",
    });
    await waitForFrameBack(toplevel, "the frame to come back when the last layer closes");
  });

  it("comes back on the rectangle measured while the menu was open", async () => {
    const before = await stageRect();
    await clickElement(toplevel, ".menubar__title--file");
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the File dropdown to open",
    });
    await waitForHidden(toplevel, "the surface to hide while the File dropdown is open");

    // A layout change with the layer open. The rectangle is measured and held, never sent: nothing
    // may raise the surface over the menu. That the surface does not move while hidden is not
    // separately visible, so what this pins is where the frame lands when it comes back.
    const shrunk = await setStageHeight("120px");
    if (shrunk >= before.height) {
      throw new Error(
        `the stage did not shrink: it is ${shrunk} px against ${before.height} before. Nothing ` +
          `below this proves anything about which rectangle the frame came back on.`,
      );
    }
    const now = await stageRect();

    pressKey("Escape");
    await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
      timeout: 15000,
      message: "the File dropdown to close",
    });
    await waitForFrameBack(toplevel, "the frame to come back after the layout changed under it");

    const near = (a, b) => Math.abs(a - b) <= TOLERANCE_PX;
    await waitFor(
      () => {
        const surface = currentSurface(toplevel);
        return near(surface.relX, now.x) &&
          near(surface.relY, now.y) &&
          near(surface.width, now.width) &&
          near(surface.height, now.height)
          ? true
          : null;
      },
      {
        timeout: 15000,
        message:
          `the surface to land on the stage as it is now (${JSON.stringify(now)}), not as it was ` +
          `before the menu opened (${JSON.stringify(before)})\n${rootTree()}`,
      },
    );
  });
});
