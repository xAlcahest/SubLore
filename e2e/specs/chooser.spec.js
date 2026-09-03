/* global describe, it, before, document, window */
/**
 * What M2.0's T1 promises about the chooser and no other spec asserts.
 *
 * The other specs prove the routes: a video, a subtitle, a copy, a project folder and an episode
 * file all arrive through the system chooser. These two things are about what T1 took away and what
 * it must not have broken along the way — no box anywhere takes a path any more, and dismissing a
 * chooser leaves the app exactly where it was on every one of the five routes.
 *
 * T7 replaced the project panel with the rail, so the three project routes are raised from the
 * rail's own menus here rather than from panel buttons. What is asserted did not move.
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { appLog } from "../lib/applog.js";
import { answerChooser, cancelChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, rightClickAt, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/**
 * The input types a person types free text into, copied from `TEXT_INPUT_TYPES` in
 * src/components/CueList.tsx. A checkbox and a slider are inputs too and neither can hold a path.
 */
const TEXT_TYPES = ["text", "search", "url", "email", "tel", "password", "number", "textarea"];

/**
 * Every box a person types into. The rail's question is open only while it is asked; the current
 * line's text box and its two time fields are in the tools column whenever a document is (T5,
 * M2.7 E1). None of them holds a path, which is what the assertion below is about.
 */
const ALLOWED_TEXT_FIELDS = [
  "currentline__text",
  "currentline__time currentline__end",
  "currentline__time currentline__start",
  "raildialog__field",
];

/** The title the episode added below carries, so the rail has a row to raise a file chooser from. */
const EPISODE_TITLE = "Episode 1";

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

async function pointAt(toplevel, selector) {
  const centre = await centreOf(selector);
  if (centre === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  return { x: toplevel.absX + centre.x, y: toplevel.absY + centre.y };
}

async function clickElement(toplevel, selector) {
  const point = await pointAt(toplevel, selector);
  clickAt(point.x, point.y);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function attributeOf(selector, name) {
  return browser.execute(
    (css, attribute) => document.querySelector(css)?.getAttribute(attribute) ?? null,
    selector,
    name,
  );
}

/** What each attached file is and where, which the rows carry as their tooltip. */
function attachedFiles() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".rail__file")).map((node) => node.getAttribute("title")),
  );
}

/** How many times the app has said it was cancelled on this route. Counted, never matched once: */
/* every spec in the run shares one log, and so does every earlier test in this file. */
function cancellations(kind) {
  const said = new RegExp(`chooser: the ${kind} choice was cancelled`, "g");
  return (appLog(dataHome()).match(said) ?? []).length;
}

/** The project node's own menu, which is where the two folder routes live (decision 24, A3). */
async function openProjectMenu(toplevel) {
  const open = await present(".rail__project");
  await clickElement(toplevel, open ? ".rail__project" : ".rail__empty");
  await waitFor(() => present(".railmenu"), {
    timeout: 15000,
    message: "the rail's project menu to open",
  });
}

/** An episode's or a file's own menu, reached the way a person reaches it. */
async function openMenuOn(toplevel, selector) {
  const point = await pointAt(toplevel, selector);
  rightClickAt(point.x, point.y);
  await waitFor(() => present(".railmenu"), {
    timeout: 15000,
    message: `the context menu on ${selector} to open`,
  });
}

async function chooseMenuItem(toplevel, key) {
  await clickElement(toplevel, `.railmenu__item--${key}`);
  await waitFor(async () => (await present(".railmenu")) === false, {
    timeout: 15000,
    message: `the menu to close after ${key}`,
  });
}

/**
 * Raise a chooser however the app raises it, dismiss it with Escape, and wait for the app to say it
 * took that as a cancellation. The app's own line is the signal: a pause here would assert on a
 * state the app has not reached yet and would pass whatever it did.
 */
async function cancelAfter(toplevel, raise, title, kind) {
  const before = cancellations(kind);
  await raise();
  const chooser = await waitForChooser(title);
  await cancelChooser(chooser, kind);
  focusWindow(toplevel.id);
  await waitFor(() => cancellations(kind) > before, {
    timeout: 15000,
    message: `the app to report the ${kind} chooser as cancelled`,
  });
}

function cancelFrom(toplevel, selector, title, kind) {
  return cancelAfter(toplevel, () => clickElement(toplevel, selector), title, kind);
}

/** Answer a chooser with a path, to reach the state the cancellations below are measured from. */
async function chooseFrom(toplevel, selector, title, chosen, what) {
  await clickElement(toplevel, selector);
  const chooser = await waitForChooser(title);
  await answerChooser(chooser, chosen, what);
  focusWindow(toplevel.id);
}

/** Choose a project folder through the menu item that raises the chooser; nothing is typed. */
async function chooseFolder(toplevel, key, folder) {
  await openProjectMenu(toplevel);
  await chooseMenuItem(toplevel, key);
  const chooser = await waitForChooser("Choose a project folder");
  await answerChooser(chooser, folder, "project folder");
  focusWindow(toplevel.id);
}

/** Type into the one question the rail asks, which opens with its field already focused. */
async function typeIntoDialog(text) {
  await waitFor(
    () => browser.execute(() => document.activeElement?.matches(".raildialog__field") === true),
    { timeout: 15000, message: "the dialog's field to take keyboard focus" },
  );
  typeText(text);
  await waitFor(
    () =>
      browser.execute((want) => document.querySelector(".raildialog__field")?.value === want, text),
    { timeout: 15000, message: `the dialog's field to hold exactly ${text}` },
  );
}

async function confirmDialog(toplevel) {
  await clickElement(toplevel, ".raildialog__confirm");
  await waitFor(async () => (await present(".raildialog")) === false, {
    timeout: 15000,
    message: "the dialog to close after it was confirmed",
  });
}

/**
 * Every spec shares one data home, so a spec that ran before this one may have left a project open,
 * and a launch now reopens it (decision 24, D5). This one starts from nothing open.
 */
async function closeAnyOpenProject(toplevel) {
  if (!(await present(".rail__project"))) {
    return;
  }
  await openProjectMenu(toplevel);
  await chooseMenuItem(toplevel, "close-project");
  await confirmDialog(toplevel);
  await waitFor(() => present(".rail__empty"), {
    timeout: 20000,
    message: "the rail to empty once another spec's project is closed",
  });
}

describe("the chooser is the only way in", () => {
  let toplevel = null;
  let projectFolder = null;
  let subtitle = null;
  let episodeFile = null;
  let saveFolder = null;

  before(async () => {
    projectFolder = scratch("series");
    saveFolder = scratch("copies");
    subtitle = path.join(scratch("media"), "ep01.srt");
    copyFileSync(fixture("srt", "clean", "basic-lf.srt"), subtitle);
    episodeFile = path.join(scratch("episode"), "ep01.srt");
    copyFileSync(fixture("srt", "clean", "basic-lf.srt"), episodeFile);

    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__file-open-subtitle") !== null),
      {
        timeout: 30000,
        message: "the app UI to render",
      },
    );
    await closeAnyOpenProject(toplevel);

    // A document, a project and an episode with a file on it, so every one of the five choosers can
    // be raised and every route below has a state to be left alone.
    await chooseFrom(
      toplevel,
      ".toolbar__file-open-subtitle",
      "Choose a subtitle",
      subtitle,
      "subtitle",
    );
    await waitFor(async () => (await textOf(".statusbar__document"))?.startsWith("SRT") === true, {
      timeout: 20000,
      message: "the status line to report the open subtitle",
    });

    await chooseFolder(toplevel, "create-project", projectFolder);
    await waitFor(() => present(".rail__project"), {
      timeout: 20000,
      message: "the rail to hold the project that was just created",
    });

    await openProjectMenu(toplevel);
    await chooseMenuItem(toplevel, "add-episode");
    await typeIntoDialog(EPISODE_TITLE);
    await confirmDialog(toplevel);
    await waitFor(() => present(".rail__episode"), {
      timeout: 20000,
      message: "the rail to hold the episode that was just added",
    });

    await openMenuOn(toplevel, ".rail__episode");
    await chooseMenuItem(toplevel, "attach-source");
    const chooser = await waitForChooser("Choose a video or subtitle file");
    await answerChooser(chooser, episodeFile, "episode file");
    focusWindow(toplevel.id);
    await waitFor(async () => (await attachedFiles()).some((line) => line?.includes(episodeFile)), {
      timeout: 20000,
      message: `the rail to hold an attachment for ${episodeFile}`,
    });
  });

  it("leaves no field in the interface that a path can be typed into", async () => {
    // The rail's question is the one box that takes typing, so it is open while this is counted:
    // asserting on a DOM that has no text field at all would pass whatever T1 had left behind.
    await openProjectMenu(toplevel);
    await chooseMenuItem(toplevel, "add-episode");
    await waitFor(() => present(".raildialog__field"), {
      timeout: 15000,
      message: "the rail's question to open",
    });

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

    // The app is holding four paths at this moment and not one of them is in a field.
    for (const field of fields) {
      for (const held of [projectFolder, subtitle, episodeFile, saveFolder]) {
        expect(field.value).not.toContain(held);
      }
    }

    await clickElement(toplevel, ".raildialog__cancel");
    await waitFor(async () => (await present(".raildialog")) === false, {
      timeout: 15000,
      message: "the rail's question to close again",
    });
  });

  it("leaves the stage alone when the video chooser is dismissed", async () => {
    const empty = await textOf(".stage__empty");
    expect(empty).not.toBe(null);

    await cancelFrom(toplevel, ".toolbar__video-open", "Choose a video", "video");

    expect(await textOf(".stage__empty")).toBe(empty);
    expect(await textOf(".statusbar__video-error")).toBe(null);
  });

  it("leaves the open document alone when the subtitle chooser is dismissed", async () => {
    const status = await textOf(".statusbar__document");

    await cancelFrom(toplevel, ".toolbar__file-open-subtitle", "Choose a subtitle", "subtitle");

    expect(await textOf(".statusbar__document")).toBe(status);
    expect(await textOf(".statusbar__error")).toBe(null);
    expect(await textOf(".statusbar__dirty")).toBe(null);
  });

  it("writes nothing when the save chooser is dismissed", async () => {
    expect(readdirSync(saveFolder)).toEqual([]);

    await cancelFrom(
      toplevel,
      ".toolbar__file-save-copy",
      "Save a copy of the subtitle",
      "subtitle-save",
    );

    expect(readdirSync(saveFolder)).toEqual([]);
    expect(await textOf(".statusbar__error")).toBe(null);
    expect(await textOf(".statusbar__dirty")).toBe(null);
  });

  it("leaves the project alone when the folder chooser is dismissed", async () => {
    const shown = await attributeOf(".rail__project", "title");
    expect(shown).toContain(projectFolder);

    await cancelAfter(
      toplevel,
      async () => {
        await openProjectMenu(toplevel);
        await chooseMenuItem(toplevel, "open-project");
      },
      "Choose a project folder",
      "project-folder",
    );

    expect(await attributeOf(".rail__project", "title")).toBe(shown);
    expect(await textOf(".statusbar__project-error")).toBe(null);
  });

  it("leaves the episode alone when the file chooser is dismissed", async () => {
    const attached = await attachedFiles();
    expect(attached).not.toHaveLength(0);

    await cancelAfter(
      toplevel,
      async () => {
        await openMenuOn(toplevel, ".rail__episode");
        await chooseMenuItem(toplevel, "attach-target");
      },
      "Choose a video or subtitle file",
      "project-file",
    );

    expect(await attachedFiles()).toEqual(attached);
    expect(await textOf(".statusbar__project-error")).toBe(null);
  });
});
