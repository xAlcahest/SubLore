/* global describe, it, before, document, window */
/**
 * Decision 7: the open subtitle document is on the video frame, in either order, and View turns it
 * off and on again.
 *
 * What is asserted, and what is not. The harness cannot read the picture: `video-surface.spec.js`
 * says in its own header why nothing in CI asserts pixels under Xvfb and llvmpipe, and that has not
 * changed. What is asserted instead is mpv's own answer about the overlay it holds, which the app
 * reads back off mpv after every change and writes into its log: how many external subtitle tracks
 * there are, whether the document's own is selected, whether it is visible, and how many characters
 * long the line at the playhead is. That is one step short of the glyphs, and it is the strongest
 * signal this harness can reach: the count only moves when mpv has really read the document that
 * was just written.
 *
 * The two fixtures open on a cue that already covers the frame a paused player shows at zero, so
 * nothing here has to seek before it can ask what is on screen.
 */
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { browser, expect } from "@wdio/globals";

import { appLog } from "../lib/applog.js";
import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import { repoRoot, requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** The first cue of each fixture, which is the line covering the frame at zero. */
const LONG_FIRST_CUE = "The ferry runs at six, not before.";
const SHORT_FIRST_CUE = "Bring the nets in.";
/** Typed over the short fixture's first cue. Its length is what proves the edit reached mpv. */
const EDITED_FIRST_CUE = "Nets in, all of them.";
/** The row the edit lands on, 1-based in the cue list. */
const FIRST_ROW = 1;

const LONG_STATUS = "SRT · 3 cues · LF";
const SHORT_STATUS = "SRT · 2 cues · LF";

function dataHome() {
  const home = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof home !== "string" || home === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  return home;
}

function fixture(name) {
  const file = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", name);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with \`git checkout fixtures/subtitles\`.`,
    );
  }
  return file;
}

/** A folder of the harness's own, so the file this spec edits is never a committed fixture. */
function workingDirectory() {
  const directory = path.join(dataHome(), "preview");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  return directory;
}

/** Everything the app has kept in its backup store, so a preview adding to it would show up. */
function backups() {
  const store = path.join(dataHome(), "com.sublore.app", "backups");
  return existsSync(store) ? readdirSync(store, { recursive: true }).sort() : [];
}

/** The last thing the app said about the overlay mpv holds, or null before it has said anything. */
function lastDrawn() {
  const lines = appLog(dataHome())
    .split("\n")
    .filter((line) => line.includes("preview: mpv holds the document"));
  return lines.at(-1) ?? null;
}

/**
 * Wait until the app's newest report about the overlay contains `expected`.
 *
 * The newest and not any: the log keeps every line, so "somewhere in the file" would let a reading
 * from before a toggle answer for the state after it.
 */
async function waitForDrawn(expected, what, timeout = 30000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const line = lastDrawn();
    if (line !== null && line.includes(expected)) {
      return line;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `the app never reported ${what} within ${timeout}ms. It last said: ${line ?? "(nothing about the overlay yet)"}`,
      );
    }
    await sleep(100);
  }
}

/** What one line of the given length looks like in the app's report. */
function drawing(chars) {
  return `external tracks 1, selected yes, visible yes, ${chars} chars at the playhead`;
}

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

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** Click the text cell of the row at a 1-based list position, which opens its inline editor. */
async function clickRow(toplevel, position) {
  const centre = await browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    const cell = row?.querySelector(".cuelist__text");
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
}

function rowText(position) {
  return browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    return row?.querySelector(".cuelist__text")?.textContent ?? null;
  }, String(position));
}

async function waitForStatus(expected) {
  return waitFor(
    async () => {
      const status = await textOf(".statusbar__document");
      return status !== null && status.startsWith(expected) ? status : null;
    },
    {
      timeout: 20000,
      message: `the subtitle status line to start with ${JSON.stringify(expected)}`,
    },
  );
}

async function openSubtitle(toplevel, file) {
  await clickElement(toplevel, ".toolbar__open-subtitle");
  const chooser = await waitForChooser("Choose a subtitle");
  await answerChooser(chooser, file, "subtitle");
  focusWindow(toplevel.id);
}

async function openVideo(toplevel, file) {
  await clickElement(toplevel, ".toolbar__open-video");
  const chooser = await waitForChooser("Choose a video");
  await answerChooser(chooser, file, "video");
  focusWindow(toplevel.id);
}

/** Whether View's own item is marked, read off the menu it lives in. */
async function subtitlesChecked(toplevel) {
  await clickElement(toplevel, ".menubar__title--view");
  await waitFor(() => present(".menubar__item--subtitle-preview"), {
    timeout: 15000,
    message: "the View menu to open on its subtitle item",
  });
  return browser.execute(
    () =>
      document.querySelector(".menubar__item--subtitle-preview")?.getAttribute("aria-checked") ===
      "true",
  );
}

/** Open View and choose the subtitle toggle. */
async function toggleSubtitles(toplevel) {
  const before = await subtitlesChecked(toplevel);
  await clickElement(toplevel, ".menubar__item--subtitle-preview");
  return !before;
}

describe("the document on the video frame", () => {
  let toplevel = null;
  let working = null;
  let longCopy = null;
  let shortCopy = null;
  let openedBytes = null;

  before(async () => {
    requireVideoFixture();
    working = workingDirectory();
    // Copies, never the committed fixtures: a preview that wrote to the file it draws from would
    // show up here as a changed copy rather than as a changed fixture in the repository.
    longCopy = path.join(working, "starts-at-zero.srt");
    shortCopy = path.join(working, "starts-at-zero-short.srt");
    copyFileSync(fixture("starts-at-zero.srt"), longCopy);
    copyFileSync(fixture("starts-at-zero-short.srt"), shortCopy);
    openedBytes = readFileSync(shortCopy);

    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(() => present(".toolbar__open-subtitle"), {
      timeout: 30000,
      message: "the app UI to render",
    });
  });

  it("puts a document that was open first onto a video opened after it", async () => {
    await openSubtitle(toplevel, longCopy);
    expect(await waitForStatus(LONG_STATUS)).toBe(LONG_STATUS);

    await openVideo(toplevel, requireVideoFixture());
    await waitFor(
      () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".controls__button")?.disabled === false,
        ),
      { timeout: 30000, message: "the video fixture to reach the ready state" },
    );

    await waitForDrawn(
      drawing(LONG_FIRST_CUE.length),
      `the ${LONG_FIRST_CUE.length} characters of the first cue on the frame`,
    );
    expect(await textOf(".statusbar__preview-error")).toBe(null);
  });

  it("puts a document opened while the video is already loaded onto the frame", async () => {
    await openSubtitle(toplevel, shortCopy);
    expect(await waitForStatus(SHORT_STATUS)).toBe(SHORT_STATUS);

    // The other order of the same pair, which is the half of the bug that had no test.
    await waitForDrawn(
      drawing(SHORT_FIRST_CUE.length),
      `the ${SHORT_FIRST_CUE.length} characters of the second fixture on the frame`,
    );
  });

  it("puts an edit on the frame without stacking a second subtitle track", async () => {
    await clickRow(toplevel, FIRST_ROW);
    await waitFor(() => present(".cuelist__editor"), {
      timeout: 15000,
      message: "the inline editor to open",
    });
    pressKey("ctrl+a");
    typeText(EDITED_FIRST_CUE);
    pressKey("Return");
    await waitFor(async () => (await rowText(FIRST_ROW)) === EDITED_FIRST_CUE, {
      timeout: 20000,
      message: `row ${FIRST_ROW} to hold the edit`,
    });

    // "external tracks 1" is the other half of the assertion: mpv re-reads the file it already has
    // rather than loading a second copy of it, so an edit does not cost a track.
    await waitForDrawn(
      drawing(EDITED_FIRST_CUE.length),
      `the edited line's ${EDITED_FIRST_CUE.length} characters on the frame, on one track`,
    );
  });

  it("takes the document off the frame from View, and puts it back", async () => {
    expect(await toggleSubtitles(toplevel)).toBe(false);
    await waitForDrawn(
      `external tracks 1, selected yes, visible no, ${EDITED_FIRST_CUE.length} chars at the playhead`,
      "the overlay turned off while mpv still holds the document",
    );

    expect(await toggleSubtitles(toplevel)).toBe(true);
    await waitForDrawn(
      drawing(EDITED_FIRST_CUE.length),
      "the overlay turned back on with the same line under it",
    );
    // Turning it back on is not a re-open: the item is marked again and nothing was reloaded.
    expect(await subtitlesChecked(toplevel)).toBe(true);
    await clickElement(toplevel, ".menubar__title--view");
  });

  it("never writes the subtitle file it is drawing from, and keeps no backup of it", async () => {
    const kept = backups();
    const before = statSync(shortCopy);

    // Everything above has already happened to this document: it was opened, drawn, edited and
    // toggled, and none of it is a save. The file is still the one that was opened.
    expect(readFileSync(shortCopy).equals(openedBytes)).toBe(true);
    expect(statSync(shortCopy).mtimeMs).toBe(before.mtimeMs);
    // Nothing new landed beside it either: a shadow copy in the user's own folder is the failure
    // this asserts against (CONTRIBUTING.md section 3).
    expect(readdirSync(working).sort()).toEqual(
      [path.basename(longCopy), path.basename(shortCopy)].sort(),
    );
    // And no backup was taken, because nothing was overwritten to make a picture.
    expect(backups()).toEqual(kept);
    expect(await present(".statusbar__dirty")).toBe(true);
  });
});
