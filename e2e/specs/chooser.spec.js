/* global describe, it, before, document, window */
/**
 * What M2.0's T1 promises about the chooser and no other spec asserts.
 *
 * The other specs prove the routes: a video, a subtitle, a copy, a project folder and an episode
 * file all arrive through the system chooser. These two things are about what T1 took away and what
 * it must not have broken along the way — no box anywhere takes a path any more, and dismissing a
 * chooser leaves the app exactly where it was on every one of the five routes.
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { appLog } from "../lib/applog.js";
import { answerChooser, cancelChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/**
 * The input types a person types free text into, copied from `TEXT_INPUT_TYPES` in
 * src/components/CueList.tsx. A checkbox and a slider are inputs too and neither can hold a path.
 */
const TEXT_TYPES = ["text", "search", "url", "email", "tel", "password", "number", "textarea"];

/** The one box T1 left that a person types into. A cue editor exists only while a cue is open. */
const ALLOWED_TEXT_FIELDS = ["project__new-episode"];

function dataHome() {
  const home = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof home !== "string" || home === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  return home;
}

/** A fresh folder under the harness temp dir. Nothing here writes anywhere else. */
function scratch(name) {
  const directory = path.join(dataHome(), "chooser", name);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  return directory;
}

/** Subtitle fixtures are committed: a missing one is a broken checkout, never a skip. */
function fixture(...parts) {
  const file = path.join(repoRoot, "fixtures", "subtitles", ...parts);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with \`git checkout fixtures/subtitles\`.`,
    );
  }
  return file;
}

/** Centre of an element in physical pixels, which is what X11 pointer coordinates are. */
function centreOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    // The rail is only as tall as the top block, so a control further down it has to be brought
    // into view before a pointer can reach it. See T2's full-width grid.
    element.scrollIntoView({ block: "nearest", inline: "nearest" });
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

/** How many times the app has said it was cancelled on this route. Counted, never matched once: */
/* every spec in the run shares one log, and so does every earlier test in this file. */
function cancellations(kind) {
  const said = new RegExp(`chooser: the ${kind} choice was cancelled`, "g");
  return (appLog(dataHome()).match(said) ?? []).length;
}

/**
 * Raise a chooser, dismiss it with Escape, and wait for the app to say it took that as a
 * cancellation. The app's own line is the signal: a pause here would assert on a state the app has
 * not reached yet and would pass whatever it did.
 */
async function cancelFrom(toplevel, selector, title, kind) {
  const before = cancellations(kind);
  await clickElement(toplevel, selector);
  const chooser = await waitForChooser(title);
  await cancelChooser(chooser, kind);
  focusWindow(toplevel.id);
  await waitFor(() => cancellations(kind) > before, {
    timeout: 15000,
    message: `the app to report the ${kind} chooser as cancelled`,
  });
}

/** Answer a chooser with a path, to reach the state the cancellations below are measured from. */
async function chooseFrom(toplevel, selector, title, chosen, what) {
  await clickElement(toplevel, selector);
  const chooser = await waitForChooser(title);
  await answerChooser(chooser, chosen, what);
  focusWindow(toplevel.id);
}

describe("the chooser is the only way in", () => {
  let toplevel = null;
  let projectFolder = null;
  let subtitle = null;
  let saveFolder = null;

  before(async () => {
    projectFolder = scratch("series");
    saveFolder = scratch("copies");
    subtitle = path.join(scratch("media"), "ep01.srt");
    copyFileSync(fixture("srt", "clean", "basic-lf.srt"), subtitle);

    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(() => browser.execute(() => document.querySelector(".subbar__open") !== null), {
      timeout: 30000,
      message: "the app UI to render",
    });

    // A document and a project, so every field the interface can hold is in the DOM and every one
    // of the five choosers can be raised.
    await chooseFrom(toplevel, ".subbar__open", "Choose a subtitle", subtitle, "subtitle");
    await waitFor(async () => (await textOf(".statusbar__document"))?.startsWith("SRT") === true, {
      timeout: 20000,
      message: "the status line to report the open subtitle",
    });
    await chooseFrom(
      toplevel,
      ".project__choose-folder",
      "Choose a project folder",
      projectFolder,
      "project folder",
    );
    await clickElement(toplevel, ".project__create");
    await waitFor(
      () => browser.execute(() => document.querySelector(".project__new-episode") !== null),
      {
        timeout: 20000,
        message: "the project panel to open",
      },
    );
  });

  it("leaves no field in the interface that a path can be typed into", async () => {
    const fields = await browser.execute(() =>
      Array.from(document.querySelectorAll("input, textarea")).map((element) => ({
        type: element.tagName === "TEXTAREA" ? "textarea" : element.type,
        className: element.className,
        value: typeof element.value === "string" ? element.value : "",
      })),
    );
    // A DOM with no inputs at all would pass every assertion below while proving nothing.
    expect(fields.length).toBeGreaterThan(0);

    const typed = fields.filter((field) => TEXT_TYPES.includes(field.type));
    expect(typed.map((field) => field.className).sort()).toEqual(ALLOWED_TEXT_FIELDS);

    // The app is holding three paths at this moment and not one of them is in a field.
    for (const field of fields) {
      for (const held of [projectFolder, subtitle, saveFolder]) {
        expect(field.value).not.toContain(held);
      }
    }
  });

  it("leaves the stage alone when the video chooser is dismissed", async () => {
    const empty = await textOf(".stage__empty");
    expect(empty).not.toBe(null);

    await cancelFrom(toplevel, ".bar__button", "Choose a video", "video");

    expect(await textOf(".stage__empty")).toBe(empty);
    expect(await textOf(".statusbar__video-error")).toBe(null);
  });

  it("leaves the open document alone when the subtitle chooser is dismissed", async () => {
    const status = await textOf(".statusbar__document");

    await cancelFrom(toplevel, ".subbar__open", "Choose a subtitle", "subtitle");

    expect(await textOf(".statusbar__document")).toBe(status);
    expect(await textOf(".statusbar__error")).toBe(null);
    expect(await textOf(".statusbar__dirty")).toBe(null);
  });

  it("writes nothing when the save chooser is dismissed", async () => {
    expect(readdirSync(saveFolder)).toEqual([]);

    await cancelFrom(
      toplevel,
      ".subbar__save-copy",
      "Save a copy of the subtitle",
      "subtitle-save",
    );

    expect(readdirSync(saveFolder)).toEqual([]);
    expect(await textOf(".statusbar__error")).toBe(null);
    expect(await textOf(".statusbar__dirty")).toBe(null);
  });

  it("leaves the project alone when the folder chooser is dismissed", async () => {
    const shown = await textOf(".project__path");
    const status = await textOf(".project__status");
    expect(shown).toContain(projectFolder);

    await cancelFrom(
      toplevel,
      ".project__choose-folder",
      "Choose a project folder",
      "project-folder",
    );

    expect(await textOf(".project__path")).toBe(shown);
    expect(await textOf(".project__status")).toBe(status);
    expect(await textOf(".project__error")).toBe(null);
  });

  it("leaves the episode alone when the file chooser is dismissed", async () => {
    const shown = await textOf(".project__file-path");

    await cancelFrom(
      toplevel,
      ".project__choose-file",
      "Choose a video or subtitle file",
      "project-file",
    );

    expect(await textOf(".project__file-path")).toBe(shown);
    expect(await textOf(".project__error")).toBe(null);
  });
});
