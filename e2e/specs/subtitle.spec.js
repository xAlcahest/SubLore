/* global describe, it, before, document, window */
import { Buffer } from "node:buffer";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** What the status line says for the clean fixtures this spec opens, one per format in v1 scope. */
const LF_STATUS = "SRT · 3 cues · LF";
const CRLF_STATUS = "SRT · 3 cues · CRLF";
const ASS_STATUS = "ASS · 3 cues · CRLF";
const VTT_STATUS = "VTT · 3 cues · LF";
/** missing-arrow.srt loses its arrow on line 6; the sidecar next to the fixture says so too. */
const MALFORMED_LINE = "Line 6";
const NO_FILE_STATUS = "No subtitle file open.";
/** The cue the discard check edits, 1-based in the list, and what it types over the text there. */
const DISCARD_POSITION = 1;
const DISCARD_TEXT = "Typed and then thrown away";

/** Subtitle fixtures are committed, unlike the video one: a missing file is a broken checkout. */
function fixture(...parts) {
  const file = path.join(repoRoot, "fixtures", "subtitles", ...parts);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with \`git checkout fixtures/subtitles\`.`,
    );
  }
  return file;
}

/** Writes go to the harness temp dir, never into the repo and never beside a fixture. */
function saveDirectory() {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "save-as");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  return directory;
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

/** The text a row shows, by 1-based list position, or null when that row is not rendered. */
function rowText(position) {
  return browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    return row?.querySelector(".cuelist__text")?.textContent ?? null;
  }, String(position));
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** A drawn control's greying, or null when the control is not drawn at all. */
function disabledOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.disabled ?? null, selector);
}

/** Open a subtitle through the system chooser, which is the only route since T1. */
async function openSubtitle(toplevel, file) {
  await clickElement(toplevel, ".toolbar__file-open-subtitle");
  const chooser = await waitForChooser("Choose a subtitle");
  await answerChooser(chooser, file, "subtitle");
  focusWindow(toplevel.id);
}

/** Name the copy in the save chooser. Its filename field is what the destination box used to be. */
async function saveCopyTo(toplevel, destination) {
  await clickElement(toplevel, ".toolbar__file-save-copy");
  const chooser = await waitForChooser("Save a copy of the subtitle");
  await answerChooser(chooser, destination, "save a copy");
  focusWindow(toplevel.id);
}

/** Save the open document elsewhere and prove the copy holds the bytes that were opened. */
async function savesIdenticalCopy(toplevel, source, saveDir) {
  const destination = path.join(saveDir, path.basename(source));

  await saveCopyTo(toplevel, destination);
  await waitFor(async () => (await textOf(".statusbar__message"))?.includes(destination) === true, {
    timeout: 20000,
    message: `the status line to report the copy at ${destination}`,
  });

  expect(await textOf(".statusbar__error")).toBe(null);
  // The point of the whole milestone: what came back out is what went in, byte for byte.
  expect(Buffer.compare(readFileSync(source), readFileSync(destination))).toBe(0);
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

describe("subtitle open and save", () => {
  let toplevel = null;
  let saveDir = null;

  before(async () => {
    saveDir = saveDirectory();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__file-open-subtitle") !== null),
      {
        timeout: 30000,
        message: "the subtitle bar to render",
      },
    );
  });

  it("opens an SRT fixture and shows its format and cue count", async () => {
    await openSubtitle(toplevel, fixture("srt", "clean", "basic-lf.srt"));

    expect(await waitForStatus(LF_STATUS)).toBe(LF_STATUS);
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("saves a byte-identical copy", async () => {
    const source = fixture("srt", "clean", "basic-crlf.srt");

    await openSubtitle(toplevel, source);
    await waitForStatus(CRLF_STATUS);

    await savesIdenticalCopy(toplevel, source, saveDir);
  });

  // SRT is not the format range: CONTRIBUTING.md section 1 puts ASS and VTT in v1 with the same
  // lossless promise, and until these two the app was only ever driven through one of the three.
  it("opens an ASS fixture and saves a byte-identical copy", async () => {
    const source = fixture("ass", "clean", "basic.ass");

    await openSubtitle(toplevel, source);
    expect(await waitForStatus(ASS_STATUS)).toBe(ASS_STATUS);
    expect(await textOf(".statusbar__error")).toBe(null);

    await savesIdenticalCopy(toplevel, source, saveDir);
  });

  it("opens a VTT fixture and saves a byte-identical copy", async () => {
    const source = fixture("vtt", "clean", "basic.vtt");

    await openSubtitle(toplevel, source);
    expect(await waitForStatus(VTT_STATUS)).toBe(VTT_STATUS);
    expect(await textOf(".statusbar__error")).toBe(null);

    await savesIdenticalCopy(toplevel, source, saveDir);
  });

  it("reports a malformed file readably and stays usable", async () => {
    await openSubtitle(toplevel, fixture("srt", "malformed", "missing-arrow.srt"));

    const message = await waitFor(
      async () => {
        const text = await textOf(".statusbar__error");
        return text !== null && text.trim() !== "" ? text : null;
      },
      { timeout: 20000, message: "the subtitle error line to appear" },
    );
    expect(message).toContain(MALFORMED_LINE);
    expect(await textOf(".statusbar__document")).toBe(NO_FILE_STATUS);

    // Still usable: the clean fixture opens straight afterwards, with the error line gone.
    await openSubtitle(toplevel, fixture("srt", "clean", "basic-lf.srt"));
    expect(await waitForStatus(LF_STATUS)).toBe(LF_STATUS);
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("throws an unsaved edit away and writes nothing when the edit is discarded", async () => {
    // The committed fixture is copied first: the file the app is pointed at here is one it may
    // legitimately write to, so a defect shows up as a changed copy rather than a changed fixture.
    const file = path.join(saveDir, "discard-basic-lf.srt");
    copyFileSync(fixture("srt", "clean", "basic-lf.srt"), file);
    const opened = readFileSync(file);

    await openSubtitle(toplevel, file);
    await waitForStatus(LF_STATUS);
    const original = await rowText(DISCARD_POSITION);
    expect(original).not.toBe(null);

    await clickRow(toplevel, DISCARD_POSITION);
    await waitFor(() => present(".cuelist__editor"), {
      timeout: 15000,
      message: "the inline editor to open",
    });
    pressKey("ctrl+a");
    typeText(DISCARD_TEXT);
    pressKey("Return");
    await waitFor(async () => (await rowText(DISCARD_POSITION)) === DISCARD_TEXT, {
      timeout: 20000,
      message: `row ${DISCARD_POSITION} to hold the edit`,
    });
    expect(await present(".statusbar__dirty")).toBe(true);

    // Discard is drawn from the start and usable only where it is meant: an open the unsaved edit
    // refused. Reopening the same file is that refusal at its plainest, and what comes back is the
    // file on disk (owner ruling 2026-09-03).
    expect(await disabledOf(".toolbar__file-discard")).toBe(true);
    await openSubtitle(toplevel, file);
    await waitFor(
      async () => ((await disabledOf(".toolbar__file-discard")) === false ? true : null),
      { timeout: 20000, message: "the discard button to come alive once the edit refused an open" },
    );
    expect(await rowText(DISCARD_POSITION)).toBe(DISCARD_TEXT);

    await clickElement(toplevel, ".toolbar__file-discard");
    await waitFor(async () => (await rowText(DISCARD_POSITION)) === original, {
      timeout: 20000,
      message: `row ${DISCARD_POSITION} to go back to the text it was opened with`,
    });
    expect(await waitForStatus(LF_STATUS)).toBe(LF_STATUS);
    expect(await present(".statusbar__dirty")).toBe(false);
    // Back to greyed, and still drawn: there is nothing left to discard.
    expect(await disabledOf(".toolbar__file-discard")).toBe(true);
    expect(await present(".statusbar__error")).toBe(false);
    // Discarding is not a write: the file is still every byte it was opened with.
    expect(readFileSync(file).equals(opened)).toBe(true);
  });
});
