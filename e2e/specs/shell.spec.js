/* global describe, it, before, document, window */
import { existsSync } from "node:fs";
import path from "node:path";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, resizeWindow } from "../lib/input.js";
import { repoRoot, requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** Percentage widths land on fractions of a pixel, and every edge here is compared to another. */
const EDGE_SLOP_PX = 1;
/** The surface travels over IPC after layout, so its geometry lags the DOM by a frame or two. */
const TOLERANCE_PX = 2;

/** The second size the layout is asserted at. `e2e-check.sh` sizes the screen to hold it exactly. */
const WIDE_WIDTH = 1920;
const WIDE_HEIGHT = 1080;

const REGIONS = {
  chrome: ".shell__chrome",
  rail: ".shell__rail",
  video: ".shell__video",
  tools: ".shell__tools",
  grid: ".shell__grid",
  status: ".statusbar",
};

const SUBTITLE = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");

/**
 * Every region's rectangle and the viewport it has to fit inside, read in one round trip so the
 * numbers all describe the same layout rather than three of them in a row.
 */
function readLayout(selectors) {
  return browser.execute((map) => {
    const regions = {};
    for (const [name, css] of Object.entries(map)) {
      const element = document.querySelector(css);
      if (element === null) {
        regions[name] = null;
        continue;
      }
      const rect = element.getBoundingClientRect();
      regions[name] = {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        width: rect.width,
        height: rect.height,
      };
    }
    return {
      regions,
      viewport: { width: window.innerWidth, height: window.innerHeight },
      documentWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      bodyWidth: document.body.scrollWidth,
    };
  }, selectors);
}

/**
 * Every element the window edge cuts through, named.
 *
 * The regression fixture for this criterion is the `Save copy to` label clipped at 1024x700, so it
 * is asserted over the whole interface and not only over the six boxes above. Sideways is absolute:
 * nothing may cross the left or the right edge. Downwards is not, because a virtualized list is
 * taller than its viewport on purpose, so an element under the bottom edge only counts when nothing
 * above it scrolls.
 */
function clippedAtWindowEdge(slop) {
  return browser.execute((allowed) => {
    const scrollsVertically = (element) => {
      for (let node = element.parentElement; node !== null; node = node.parentElement) {
        // Computed overflow, not scrollHeight: `overflow: hidden` overflows without scrolling, and
        // reading the height alone excused every box under the window edge.
        const overflow = window.getComputedStyle(node).overflowY;
        if (
          (overflow === "auto" || overflow === "scroll") &&
          node.scrollHeight > node.clientHeight
        ) {
          return true;
        }
      }
      return false;
    };
    const clipped = [];
    for (const element of document.querySelectorAll("*")) {
      const rect = element.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        continue;
      }
      const name = `${element.tagName.toLowerCase()}.${element.className}`;
      if (rect.left < -allowed || rect.right > window.innerWidth + allowed) {
        clipped.push(`${name} spans ${Math.round(rect.left)}..${Math.round(rect.right)} across`);
      } else if (rect.bottom > window.innerHeight + allowed && !scrollsVertically(element)) {
        clipped.push(`${name} ends ${Math.round(rect.bottom)} down`);
      }
    }
    return clipped;
  }, slop);
}

/**
 * Whether anything between the video surface and the page root can scroll.
 *
 * The M0.2 constraint is about the whole chain and not about one element: the surface is placed
 * from the DOM rectangle and recomputed on resize, so an ancestor that scrolls moves the frame
 * without telling anyone. Reported as a list, so a failure names the element that grew one.
 */
function scrollableAncestorsOfSurface() {
  return browser.execute(() => {
    const start = document.querySelector(".stage__surface");
    if (start === null) {
      return null;
    }
    const scrollable = [];
    for (let node = start; node !== null; node = node.parentElement) {
      if (node.scrollHeight > node.clientHeight || node.scrollWidth > node.clientWidth) {
        scrollable.push(`${node.tagName.toLowerCase()}.${node.className}`);
      }
    }
    return scrollable;
  });
}

/**
 * The tools column's own children, each one's edges measured against the column's.
 *
 * "No empty waveform placeholder" is asserted as "nothing else takes space in this column", never
 * as "the waveform selector is missing": a panel shipped under another name would walk straight
 * past the second reading and not past this one. See T5.
 */
function toolsColumn() {
  return browser.execute(() => {
    const column = document.querySelector(".shell__tools");
    if (column === null) {
      return null;
    }
    const box = column.getBoundingClientRect();
    return {
      height: box.height,
      children: Array.from(column.children).map((child) => {
        const rect = child.getBoundingClientRect();
        return {
          name: `${child.tagName.toLowerCase()}.${child.className}`,
          top: rect.top - box.top,
          bottom: rect.bottom - box.top,
        };
      }),
    };
  });
}

/** The rectangle the native surface should be sitting on, in the physical pixels X reports. */
async function surfaceRect() {
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
function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

async function waitForSurfaceOnStage(toplevel) {
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

/** Resize the app window and wait until the page has been laid out at the new width. */
async function resizeTo(id, width, height) {
  resizeWindow(id, width, height);
  await waitFor(
    async () => ((await browser.execute(() => window.innerWidth)) === width ? 1 : null),
    {
      timeout: 15000,
      message: `the page to be laid out ${width} CSS pixels wide`,
    },
  );
}

/**
 * Assert the arrangement shell-layout.md draws, at whatever size the window is now.
 *
 * Named edges rather than pixel values: the layout is a set of relations between the regions, and a
 * check written against coordinates would be rewritten at the second size and again at T9.
 */
async function expectLayoutHolds(size) {
  const at = `at ${size.width}x${size.height}`;
  const layout = await readLayout(REGIONS);
  const missing = Object.entries(layout.regions)
    .filter(([, rect]) => rect === null)
    .map(([name]) => name);
  if (missing.length > 0) {
    throw new Error(`these regions are missing from the DOM ${at}: ${missing.join(", ")}`);
  }
  const { chrome, rail, video, tools, grid, status } = layout.regions;
  const viewport = layout.viewport;
  const near = (a, b) => Math.abs(a - b) <= EDGE_SLOP_PX;

  expect({ at, width: viewport.width, height: viewport.height }).toEqual({
    at,
    width: size.width,
    height: size.height,
  });

  for (const [name, rect] of Object.entries(layout.regions)) {
    // A collapsed region satisfies every relation below while showing nothing.
    expect({ name, at, holdsSpace: rect.width > 0 && rect.height > 0 }).toEqual({
      name,
      at,
      holdsSpace: true,
    });
    // Nothing clipped at a window edge.
    expect({
      name,
      at,
      inside:
        rect.left >= -EDGE_SLOP_PX &&
        rect.top >= -EDGE_SLOP_PX &&
        rect.right <= viewport.width + EDGE_SLOP_PX &&
        rect.bottom <= viewport.height + EDGE_SLOP_PX,
    }).toEqual({ name, at, inside: true });
  }

  // Chrome across the top, status bar across the bottom, outside the five regions.
  expect(near(chrome.top, 0)).toBe(true);
  expect(near(chrome.left, 0)).toBe(true);
  expect(near(chrome.right, viewport.width)).toBe(true);
  expect(near(status.left, 0)).toBe(true);
  expect(near(status.right, viewport.width)).toBe(true);
  expect(near(status.bottom, viewport.height)).toBe(true);

  // The rail on the left, under the chrome, beside the top block and no further down than it.
  expect(near(rail.left, 0)).toBe(true);
  expect(near(rail.top, chrome.bottom)).toBe(true);
  expect(near(rail.bottom, video.bottom)).toBe(true);

  // The video beside the rail, at the top of the block.
  expect(near(video.left, rail.right)).toBe(true);
  expect(near(video.top, rail.top)).toBe(true);

  // The tools column to the video's right, the same height, and not a pixel of it under the video.
  // This is the criterion "the current-line band is not full width under video and waveform".
  expect(near(tools.left, video.right)).toBe(true);
  expect(near(tools.top, video.top)).toBe(true);
  expect(near(tools.bottom, video.bottom)).toBe(true);
  expect(near(tools.right, viewport.width)).toBe(true);
  expect(tools.width).toBeLessThan(viewport.width - rail.width);

  // The grid below the top block and across the whole width, crossing under the rail, taking what
  // is left above the status bar. Owner ruling 2026-09-02.
  expect(near(grid.top, video.bottom)).toBe(true);
  expect(near(grid.left, 0)).toBe(true);
  expect(near(grid.right, viewport.width)).toBe(true);
  expect(near(grid.bottom, status.top)).toBe(true);

  // Nothing anywhere in the interface is cut by a window edge, and the page never scrolls sideways.
  expect({ at, clipped: await clippedAtWindowEdge(EDGE_SLOP_PX) }).toEqual({ at, clipped: [] });
  expect(layout.documentWidth).toBeLessThanOrEqual(layout.clientWidth);
  expect(layout.bodyWidth).toBeLessThanOrEqual(layout.clientWidth);
}

describe("the shell layout", () => {
  let toplevel = null;

  before(async () => {
    if (!existsSync(SUBTITLE)) {
      throw new Error(
        `E2E prerequisite missing: ${SUBTITLE} does not exist. It is committed; restore it with ` +
          "`git checkout fixtures/subtitles`.",
      );
    }
    toplevel = await waitFor(findToplevel, {
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

    // The criterion is stated with a video and a subtitle open, so both are open before it is read.
    await clickElement(toplevel, ".toolbar__open-video");
    const videoChooser = await waitForChooser("Choose a video");
    await answerChooser(videoChooser, requireVideoFixture(), "video");
    focusWindow(toplevel.id);
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".controls__button")?.disabled === false,
        ),
      { timeout: 30000, timeoutMsg: "the video fixture never reached the ready state" },
    );

    await clickElement(toplevel, ".toolbar__open-subtitle");
    const subtitleChooser = await waitForChooser("Choose a subtitle");
    await answerChooser(subtitleChooser, SUBTITLE, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(
      async () => {
        const line = await browser.execute(
          () => document.querySelector(".statusbar__document")?.textContent ?? null,
        );
        return line !== null && line.startsWith("SRT") ? line : null;
      },
      { timeout: 20000, message: "the status bar to report the open subtitle" },
    );
  });

  it("has a clipping sweep that sees a box hanging over the window edge", async () => {
    // The window will not go below 800x600 and nothing clips there either, so the only way to show
    // that the sweep below can fail is to hang something over the edge and watch it be named.
    await browser.execute(() => {
      const probe = document.createElement("div");
      probe.className = "probe";
      probe.style.cssText = "position:fixed;left:0;top:0;width:99999px;height:8px";
      document.body.append(probe);
    });
    const seen = await clippedAtWindowEdge(EDGE_SLOP_PX);
    await browser.execute(() => {
      document.querySelector(".probe")?.remove();
    });

    expect(seen.filter((entry) => entry.startsWith("div.probe")).length).toBe(1);
    expect(await clippedAtWindowEdge(EDGE_SLOP_PX)).toEqual([]);
  });

  it("has a clipping sweep that sees a box hanging under the window edge", async () => {
    // The downward arm has its own case because it has its own excuse: a box under a scroller is
    // meant to hang there. The probe sits under an `overflow: hidden` box that overflows, which is
    // what the sweep used to read as scrolling.
    await browser.execute(() => {
      const box = document.createElement("div");
      box.className = "deep-probe";
      box.style.cssText = "position:fixed;left:0;bottom:0;width:8px;height:8px;overflow:hidden";
      const inner = document.createElement("div");
      inner.className = "deep-probe__inner";
      inner.style.cssText = "height:64px";
      box.append(inner);
      document.body.append(box);
    });
    const seen = await clippedAtWindowEdge(EDGE_SLOP_PX);
    await browser.execute(() => {
      document.querySelector(".deep-probe")?.remove();
    });

    expect(seen.filter((entry) => entry.startsWith("div.deep-probe__inner")).length).toBe(1);
    expect(await clippedAtWindowEdge(EDGE_SLOP_PX)).toEqual([]);
  });

  it("puts the five regions where the layout says, at the size the app opens at", async () => {
    await expectLayoutHolds({ width: windowWidth, height: windowHeight });
  });

  it("keeps them there when the window is 1920x1080", async () => {
    await resizeTo(toplevel.id, WIDE_WIDTH, WIDE_HEIGHT);

    await expectLayoutHolds({ width: WIDE_WIDTH, height: WIDE_HEIGHT });
  });

  it("never scrolls the panel holding the video, and moves the surface when it resizes", async () => {
    // Wide: the surface is on the rectangle the resized stage reports, not on the one before it.
    const wide = findToplevel({ width: WIDE_WIDTH, height: WIDE_HEIGHT });
    if (wide === null) {
      throw new Error(`no ${WIDE_WIDTH}x${WIDE_HEIGHT} "Sublore" toplevel.\n${rootTree()}`);
    }
    await waitForSurfaceOnStage(wide);
    expect(await scrollableAncestorsOfSurface()).toEqual([]);

    await resizeTo(wide.id, windowWidth, windowHeight);

    const narrow = findToplevel();
    if (narrow === null) {
      throw new Error(`no ${windowWidth}x${windowHeight} "Sublore" toplevel.\n${rootTree()}`);
    }
    await waitForSurfaceOnStage(narrow);
    expect(await scrollableAncestorsOfSurface()).toEqual([]);
  });

  it("moves the surface when the chrome above it changes height", async () => {
    // A taller chrome slides the stage down without changing its size, which is the M0.2 constraint
    // broken by a route that never scrolls and never resizes the panel.
    const narrow = findToplevel();
    if (narrow === null) {
      throw new Error(`no ${windowWidth}x${windowHeight} "Sublore" toplevel.\n${rootTree()}`);
    }
    const before = await surfaceRect();
    await browser.execute(() => {
      const spacer = document.createElement("div");
      spacer.className = "chrome-probe";
      spacer.style.cssText = "height:64px";
      document.querySelector(".shell__chrome")?.append(spacer);
    });

    const moved = await surfaceRect();
    expect(moved.y - before.y).toBeGreaterThan(0);
    expect(moved.height).toBe(before.height);
    await waitForSurfaceOnStage(narrow);

    await browser.execute(() => {
      document.querySelector(".chrome-probe")?.remove();
    });

    expect(await surfaceRect()).toEqual(before);
    await waitForSurfaceOnStage(narrow);
  });

  // The tools column holds exactly the panels that have something to show, and nothing else. T5
  // wrote this to assert the waveform's absence; W5 gave the waveform a provider, so the panel it
  // names is now expected and the absence rule moved to `waveform.spec.js`, which asserts it in the
  // state where it still holds: before any video is open. An absence is only worth asserting if it
  // can be shown failing, so the probe goes in first and the reading is proved to name it.
  it("holds exactly the panels with something to show, and nothing else", async () => {
    await browser.execute(() => {
      const probe = document.createElement("div");
      probe.className = "waveform-probe";
      probe.style.cssText = "flex:none;height:40px";
      document.querySelector(".shell__tools")?.prepend(probe);
    });
    const withProbe = await toolsColumn();
    await browser.execute(() => {
      document.querySelector(".waveform-probe")?.remove();
    });
    expect(withProbe).not.toBe(null);
    expect(withProbe.children.map((child) => child.name)).toEqual([
      "div.waveform-probe",
      "section.waveform",
      "div.sash",
      "section.currentline",
    ]);
    expect(withProbe.children[1].top).toBeGreaterThan(EDGE_SLOP_PX);

    // Waited for, not assumed present: the waveform arrives with the peak job's first chunk, and on
    // a slower machine that is after this point. Reading the column without waiting made this
    // assertion depend on how fast the runner is, which is the defect e2e/README.md's own rule about
    // environment-anchored assertions exists to catch.
    await waitFor(() => present(".waveform"), {
      timeout: 30000,
      message: "the waveform panel to arrive before the column is read",
    });

    const column = await toolsColumn();
    expect(column.children.map((child) => child.name)).toEqual([
      "section.waveform",
      "div.sash",
      "section.currentline",
    ]);
    // Flush against both edges of the column, so nothing above or below holds any space either.
    expect(Math.abs(column.children[0].top)).toBeLessThanOrEqual(EDGE_SLOP_PX);
    expect(
      Math.abs(column.children[column.children.length - 1].bottom - column.height),
    ).toBeLessThanOrEqual(EDGE_SLOP_PX);
    expect(column.height).toBeGreaterThan(0);
  });
});
