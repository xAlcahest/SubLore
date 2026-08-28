/* global describe, it, before, document, window */
import { Buffer } from "node:buffer";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { clickAt, focusWindow, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** What the status line says for the two clean SRT fixtures this spec opens. */
const LF_STATUS = "SRT · 3 cues · LF";
const CRLF_STATUS = "SRT · 3 cues · CRLF";
/** missing-arrow.srt loses its arrow on line 6; the sidecar next to the fixture says so too. */
const MALFORMED_LINE = "Line 6";
const NO_FILE_STATUS = "No subtitle file open.";

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

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

/**
 * Replace whatever a text field holds. This spec opens four files through one field, so the old
 * path has to go; `e2e/lib/input.js` is frozen for M1, hence the local ctrl+a.
 */
async function typeInto(toplevel, selector, text) {
  await clickElement(toplevel, selector);
  await waitFor(
    () => browser.execute((css) => document.activeElement?.matches(css) === true, selector),
    { timeout: 10000, message: `${selector} to take keyboard focus` },
  );

  execFileSync("xdotool", ["key", "--clearmodifiers", "ctrl+a"], {
    encoding: "utf8",
    timeout: 15000,
  });
  typeText(text);
  // Also proves the select-all landed: leftover text would make this value wrong, not just longer.
  await waitFor(
    () =>
      browser.execute((css, want) => document.querySelector(css)?.value === want, selector, text),
    { timeout: 15000, message: `${selector} to hold exactly ${text}` },
  );
}

async function waitForStatus(expected) {
  return waitFor(
    async () => {
      const status = await textOf(".subbar__status");
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
    await waitFor(() => browser.execute(() => document.querySelector(".subbar__input") !== null), {
      timeout: 30000,
      message: "the subtitle bar to render",
    });
  });

  it("opens an SRT fixture and shows its format and cue count", async () => {
    await typeInto(toplevel, ".subbar__input", fixture("srt", "clean", "basic-lf.srt"));
    await clickElement(toplevel, ".subbar__open");

    expect(await waitForStatus(LF_STATUS)).toBe(LF_STATUS);
    expect(await textOf(".subbar__error")).toBe(null);
  });

  it("saves a byte-identical copy", async () => {
    const source = fixture("srt", "clean", "basic-crlf.srt");
    const destination = path.join(saveDir, "basic-crlf.srt");

    await typeInto(toplevel, ".subbar__input", source);
    await clickElement(toplevel, ".subbar__open");
    await waitForStatus(CRLF_STATUS);

    await typeInto(toplevel, ".subbar__dest", destination);
    await clickElement(toplevel, ".subbar__save");
    await waitFor(async () => (await textOf(".subbar__status"))?.includes(destination) === true, {
      timeout: 20000,
      message: `the status line to report the copy at ${destination}`,
    });

    expect(await textOf(".subbar__error")).toBe(null);
    // The point of the whole milestone: what came back out is what went in, byte for byte.
    expect(Buffer.compare(readFileSync(source), readFileSync(destination))).toBe(0);
  });

  it("reports a malformed file readably and stays usable", async () => {
    await typeInto(toplevel, ".subbar__input", fixture("srt", "malformed", "missing-arrow.srt"));
    await clickElement(toplevel, ".subbar__open");

    const message = await waitFor(
      async () => {
        const text = await textOf(".subbar__error");
        return text !== null && text.trim() !== "" ? text : null;
      },
      { timeout: 20000, message: "the subtitle error line to appear" },
    );
    expect(message).toContain(MALFORMED_LINE);
    expect(await textOf(".subbar__status")).toBe(NO_FILE_STATUS);

    // Still usable: the clean fixture opens straight afterwards, with the error line gone.
    await typeInto(toplevel, ".subbar__input", fixture("srt", "clean", "basic-lf.srt"));
    await clickElement(toplevel, ".subbar__open");
    expect(await waitForStatus(LF_STATUS)).toBe(LF_STATUS);
    expect(await textOf(".subbar__error")).toBe(null);
  });
});
