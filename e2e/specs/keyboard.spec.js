/* global describe, it, before, after, document, window, Event */
/**
 * K3 and K4: one dispatcher, and the four Ctrl+digit shortcuts the window context already owed.
 *
 * The four commands here already had a menu route, proved in playhead.spec.js. What is new is that
 * a key reaches them, and that the key which reaches them is the one the menu draws. Both halves
 * are asserted, because a shortcut drawn and not wired is the drift K1 existed to remove.
 *
 * The last check switches the X keymap. It is the only way to tell `event.key` from `event.code`
 * from outside the app: the same physical key reads `1` under `us` and `&` under `fr`, and a
 * dispatcher matching digits on `key` would be dead under the second. The layout is restored in an
 * `after` hook and the restore is read back, because every spec in a run shares one X server and a
 * layout left behind would make the rest of them type nonsense. See docs/keyboard-tasks.md.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey } from "../lib/input.js";
import { repoRoot, requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { closeAnyOpenProject } from "../lib/rail.js";
import { findToplevel } from "../lib/x11.js";

const OPEN_STATUS = "SRT · 3 cues · LF";
/** The fixture's own third cue, committed and byte-frozen, so these are facts. */
const THIRD_START = "00:00:09.100";
const THIRD_END = "00:00:11.760";
/** Inside the second cue, which runs 5.000 to 8.340: a moment neither boundary already holds. */
const INSIDE_SECOND = 6.5;
/** Far from every boundary these checks land on, so a key that does nothing cannot read as a pass. */
const ELSEWHERE = 2;

/**
 * The window context's Ctrl+digit bindings, from interface-spec 10.4. Written down here so that a
 * command drawing a key it does not answer on fails as loudly as one answering a key it never drew.
 */
const BOUND = [
  { token: "video-to-cue-start", key: "Ctrl+1" },
  { token: "video-to-cue-end", key: "Ctrl+2" },
  { token: "time-start-to-playhead", key: "Ctrl+3" },
  { token: "time-end-to-playhead", key: "Ctrl+4" },
];

function dataHome() {
  const home = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof home !== "string" || home === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  return home;
}

/** Writes go to the harness temp dir. The committed fixture is copied, never opened for editing. */
function workingCopy() {
  const source = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
  if (!existsSync(source)) {
    throw new Error(
      `E2E prerequisite missing: ${source} does not exist. It is committed; restore it with ` +
        "`git checkout fixtures/subtitles`.",
    );
  }
  const directory = path.join(dataHome(), "keyboard");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "basic-lf.srt");
  copyFileSync(source, copy);
  return copy;
}

/** The X keymap this run's own server carries. Read back rather than assumed, in both directions. */
function setLayout(layout) {
  try {
    execFileSync("setxkbmap", [layout], { timeout: 10000 });
  } catch (error) {
    throw new Error(
      `setxkbmap could not select the ${layout} layout (${error.message}). It comes from ` +
        "x11-xkb-utils; without it this check cannot tell event.key from event.code.",
    );
  }
  const query = execFileSync("setxkbmap", ["-query"], { encoding: "utf8", timeout: 10000 });
  const chosen = /^layout:\s*(\S+)$/m.exec(query);
  if (chosen === null || chosen[1] !== layout) {
    throw new Error(
      `the X server is still on ${chosen?.[1] ?? "an unreadable layout"} after being asked for ` +
        `${layout}. Every spec in this run shares this server, so leaving it here would make the ` +
        `rest of them type nonsense.\n${query}`,
    );
  }
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

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** Every row the grid draws, with the cursor marked, read in one round trip. */
function gridRows() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".cuelist__row")).map((row) => ({
      start: row.querySelector(".cuelist__start")?.textContent ?? null,
      end: row.querySelector(".cuelist__end")?.textContent ?? null,
      cursor: row.classList.contains("cuelist__row--active"),
    })),
  );
}

/** Where the player says it is, in seconds, off its own transport rather than off any state. */
function playhead() {
  return browser.execute(() => Number(document.querySelector(".controls__slider")?.value ?? -1));
}

/** The same instant as the grid draws it, so the two can be compared as strings. */
function asTimecode(seconds) {
  const total = Math.max(0, Math.floor(seconds * 1000));
  const pad = (value, width) => String(value).padStart(width, "0");
  return (
    `${pad(Math.floor(total / 3_600_000), 2)}:` +
    `${pad(Math.floor(total / 60_000) % 60, 2)}:` +
    `${pad(Math.floor(total / 1000) % 60, 2)}.${pad(total % 1000, 3)}`
  );
}

async function seekTo(seconds) {
  await browser.execute((target) => {
    const slider = document.querySelector(".controls__slider");
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    setter.call(slider, String(target));
    slider.dispatchEvent(new Event("input", { bubbles: true }));
    slider.dispatchEvent(new Event("change", { bubbles: true }));
  }, seconds);
  await browser.pause(300);
}

/** Put the cursor on a row by clicking its number cell, which never opens an editor. */
async function cursorTo(toplevel, position) {
  const centre = await browser.execute((wanted) => {
    const row = Array.from(document.querySelectorAll(".cuelist__row")).find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    const cell = row?.querySelector(".cuelist__pos");
    if (!cell) {
      return null;
    }
    const rect = cell.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, String(position));
  if (centre === null) {
    throw new Error(`row ${position} is missing from the DOM`);
  }
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
  await waitFor(async () => ((await gridRows())[position - 1]?.cursor === true ? true : null), {
    timeout: 15000,
    message: `the cursor to reach row ${position}`,
  });
}

async function waitForPlayhead(want) {
  const reached = await waitFor(
    async () => {
      const now = asTimecode(await playhead());
      return now === want ? now : null;
    },
    { timeout: 20000, message: `the player to reach ${want}` },
  );
  expect(reached).toBe(want);
}

describe("the shortcut is the one the menu draws", () => {
  let toplevel = null;
  let copy = null;

  before(async () => {
    copy = workingCopy();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__file-open-subtitle") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
    // One data home for the whole run, so the emptiest state is one this file makes. See N19.
    await closeAnyOpenProject(toplevel);

    await clickElement(toplevel, ".toolbar__file-open-subtitle");
    const subtitle = await waitForChooser("Choose a subtitle");
    await answerChooser(subtitle, copy, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__document"))?.includes(OPEN_STATUS) === true,
      { timeout: 20000, message: "the status bar to report the open subtitle" },
    );

    await clickElement(toplevel, ".toolbar__video-open");
    const video = await waitForChooser("Choose a video");
    await answerChooser(video, requireVideoFixture(), "video");
    focusWindow(toplevel.id);
    await waitFor(
      () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".controls__button")?.disabled === false,
        ),
      { timeout: 40000, message: "the video to be ready to play" },
    );
  });

  after(() => {
    // Unconditional: a failed check must not leave the next spec typing on a French keyboard.
    setLayout("us");
  });

  it("draws a key beside each of the four the window context binds", async () => {
    await clickElement(toplevel, ".menubar__title--timing");
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the Timing menu to open",
    });
    for (const { token, key } of BOUND) {
      const drawn = await browser.execute(
        (css) => document.querySelector(css)?.textContent ?? null,
        `.menubar__item--${token} .menubar__accelerator`,
      );
      expect({ token, drawn }).toEqual({ token, drawn: key });
    }
    pressKey("Escape");
    await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
      timeout: 15000,
      message: "the menu to close",
    });
  });

  it("sets the cursor's cue to start where the video is, on ctrl+3", async () => {
    await cursorTo(toplevel, 3);
    await seekTo(INSIDE_SECOND);
    const paused = await playhead();

    pressKey("ctrl+3");
    const want = asTimecode(paused);
    const row = await waitFor(
      async () => {
        const third = (await gridRows())[2];
        return third?.start === want ? third : null;
      },
      { timeout: 20000, message: `row 3 to start at ${want}, where the player is` },
    );
    // The other boundary is untouched, which is what says the key ran one command and not two.
    expect(row.end).toBe(THIRD_END);
  });

  it("takes the video back to that start, on ctrl+1", async () => {
    const start = (await gridRows())[2].start;
    await seekTo(ELSEWHERE);
    // Parked away from the target first: a key that does nothing must not read as a key that worked.
    expect(asTimecode(await playhead())).not.toBe(start);

    pressKey("ctrl+1");
    await waitForPlayhead(start);
  });

  it("reads the physical key rather than the glyph the layout prints on it", async () => {
    const start = (await gridRows())[2].start;
    // The start the checks above moved, so this one is reading their work and not the fixture's.
    expect(start).not.toBe(THIRD_START);
    await seekTo(ELSEWHERE);
    expect(asTimecode(await playhead())).not.toBe(start);

    setLayout("fr");
    // The key that carries `1` under `us` carries `&` under `fr`, unshifted. Same physical key,
    // same `code`, and the only press that can tell the two matching rules apart.
    pressKey("ctrl+ampersand");
    await waitForPlayhead(start);
    setLayout("us");
  });
});
