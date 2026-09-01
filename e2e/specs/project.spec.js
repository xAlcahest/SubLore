/* global describe, it, before, document, window */
import { Buffer } from "node:buffer";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** Copies of src/i18n/en.ts. The harness cannot import TypeScript, so the strings are pinned here. */
const NO_PROJECT = "No project open.";
const NO_PROJECT_HERE = "There is no Sublore project in that folder.";
const NO_SUCH_FILE = "There is no file at that path.";
const EPISODE_TITLE = "Episode 1";
/** What the episode row reads, from `en.project.episode`: "{ordinal}. {title}". */
const EPISODE_ROW = `1. ${EPISODE_TITLE}`;

/** The one file Sublore writes into a project folder, from crates/sublore-project/src/layout.rs. */
const DATABASE_NAME = "project.sublore";

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

/** Everything this spec writes lives under the harness temp dir, never in the repo. */
function scratch(name) {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "project", name);
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

function textsOf(selector) {
  return browser.execute(
    (css) => Array.from(document.querySelectorAll(css)).map((node) => node.textContent),
    selector,
  );
}

/** Choose the project folder. The panel shows what the chooser answered; nothing is typed. */
async function chooseFolder(toplevel, folder) {
  await clickElement(toplevel, ".project__choose-folder");
  const chooser = await waitForChooser("Choose a project folder");
  await answerChooser(chooser, folder, "project folder");
  focusWindow(toplevel.id);
}

/** Choose the file an episode is attached to, through the same chooser the app raises. */
async function chooseFile(toplevel, file) {
  await clickElement(toplevel, ".project__choose-file");
  const chooser = await waitForChooser("Choose a video or subtitle file");
  await answerChooser(chooser, file, "episode file");
  focusWindow(toplevel.id);
}

/** Replace whatever the episode field holds, which is the one box T1 left in the panel. */
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
      const status = await textOf(".project__status");
      return status !== null && status.includes(expected) ? status : null;
    },
    { timeout: 20000, message: `the project status line to contain ${JSON.stringify(expected)}` },
  );
}

async function waitForError() {
  return waitFor(
    async () => {
      const text = await textOf(".project__error");
      return text !== null && text.trim() !== "" ? text : null;
    },
    { timeout: 20000, message: "the project error line to appear" },
  );
}

/** The app window, after a launch or a relaunch. Also proves exactly one instance is running. */
async function attachToApp() {
  const toplevel = await waitFor(findToplevel, {
    timeout: 40000,
    message: `exactly one ${windowWidth}x${windowHeight} "Sublore" toplevel to be on screen`,
  });
  focusWindow(toplevel.id);
  await waitFor(() => browser.execute(() => document.querySelector(".project__path") !== null), {
    timeout: 40000,
    message: "the project panel to render",
  });
  return toplevel;
}

describe("projects", () => {
  let toplevel = null;
  let projectFolder = null;
  let userFolder = null;
  let emptyFolder = null;
  let userSubtitle = null;
  let sourceFixture = null;

  before(async () => {
    sourceFixture = fixture("srt", "clean", "basic-lf.srt");
    projectFolder = scratch("series");
    userFolder = scratch("user-media");
    emptyFolder = scratch("not-a-project");
    // A copy outside the project folder, so the deletion test asserts on a real user file rather
    // than on a repo fixture the app must never be able to reach.
    userSubtitle = path.join(userFolder, "ep01.srt");
    copyFileSync(sourceFixture, userSubtitle);

    toplevel = await attachToApp();
  });

  it("creates a project in an empty folder", async () => {
    expect(await textOf(".project__status")).toBe(NO_PROJECT);

    await chooseFolder(toplevel, projectFolder);
    await clickElement(toplevel, ".project__create");

    expect(await waitForStatus(projectFolder)).toContain(projectFolder);
    expect(await textOf(".project__error")).toBe(null);
    expect(existsSync(path.join(projectFolder, DATABASE_NAME))).toBe(true);
  });

  it("adds an episode and attaches a subtitle file to it", async () => {
    const before_ = statSync(userSubtitle);

    await typeInto(toplevel, ".project__new-episode", EPISODE_TITLE);
    await clickElement(toplevel, ".project__add-episode");
    await waitFor(
      async () => (await textsOf(".project__episode")).some((t) => t?.includes(EPISODE_ROW)),
      {
        timeout: 20000,
        message: `the episode list to hold ${JSON.stringify(EPISODE_ROW)}`,
      },
    );

    await chooseFile(toplevel, userSubtitle);
    await clickElement(toplevel, ".project__attach");
    await waitFor(
      async () => (await textsOf(".project__file")).some((t) => t?.includes(userSubtitle)),
      {
        timeout: 20000,
        message: `the file list to hold ${userSubtitle}`,
      },
    );

    expect(await textOf(".project__error")).toBe(null);
    // Attaching records a path. CONTRIBUTING.md §3.1: the user's file is not read for writing, not
    // copied, not moved, not touched.
    const after = statSync(userSubtitle);
    expect(after.size).toBe(before_.size);
    expect(after.mtimeMs).toBe(before_.mtimeMs);
    expect(Buffer.compare(readFileSync(userSubtitle), readFileSync(sourceFixture))).toBe(0);
  });

  it("still lists the episode and its file after the app is restarted", async () => {
    // A real relaunch: the WebDriver session is deleted, which ends the process, and the next one
    // starts the binary again. Nothing of the old run survives but the folder on disk.
    await browser.reloadSession();
    toplevel = await attachToApp();

    // Proof the app really restarted rather than kept its state: a fresh Sublore opens nothing.
    expect(await textOf(".project__status")).toBe(NO_PROJECT);
    expect(await textsOf(".project__episode")).toEqual([]);

    await chooseFolder(toplevel, projectFolder);
    await clickElement(toplevel, ".project__open");

    await waitForStatus(projectFolder);
    const episodes = await waitFor(
      async () => {
        const rows = await textsOf(".project__episode");
        return rows.length > 0 ? rows : null;
      },
      { timeout: 20000, message: "the reopened project to list its episodes" },
    );
    expect(episodes).toHaveLength(1);
    expect(episodes[0]).toContain(EPISODE_ROW);
    expect((await textsOf(".project__file")).join("\n")).toContain(userSubtitle);
    expect(await textOf(".project__error")).toBe(null);
  });

  it("reports a folder that holds no project and stays usable", async () => {
    await chooseFolder(toplevel, emptyFolder);
    await clickElement(toplevel, ".project__open");

    expect(await waitForError()).toContain(NO_PROJECT_HERE);
    expect(await textOf(".project__status")).toBe(NO_PROJECT);
    expect(existsSync(path.join(emptyFolder, DATABASE_NAME))).toBe(false);

    // Still usable: the real project opens straight afterwards, with the error line gone.
    await chooseFolder(toplevel, projectFolder);
    await clickElement(toplevel, ".project__open");
    await waitForStatus(projectFolder);
    expect(await textOf(".project__error")).toBe(null);
    expect((await textsOf(".project__episode"))[0]).toContain(EPISODE_ROW);

    // An attach that fails changed nothing, so the project stays on screen beside the message.
    // The chooser will not offer a file that is not there, so the file goes after it is chosen:
    // a file can vanish between the choosing and the attaching, and that is the case this covers.
    const vanishing = path.join(userFolder, "vanishing.srt");
    writeFileSync(vanishing, "1\n00:00:01,000 --> 00:00:02,000\nGone by then.\n");
    await chooseFile(toplevel, vanishing);
    rmSync(vanishing);
    await clickElement(toplevel, ".project__attach");
    expect(await waitForError()).toContain(NO_SUCH_FILE);
    expect(await textOf(".project__status")).toContain(projectFolder);
    expect((await textsOf(".project__episode"))[0]).toContain(EPISODE_ROW);
  });

  it("deletes the project without touching the files it points at", async () => {
    await clickElement(toplevel, ".project__delete");

    await waitFor(() => !existsSync(path.join(projectFolder, DATABASE_NAME)), {
      timeout: 20000,
      message: `${DATABASE_NAME} to be gone from ${projectFolder}`,
    });
    expect(await waitForStatus(projectFolder)).toContain(projectFolder);
    expect(await textOf(".project__error")).toBe(null);
    expect(await textsOf(".project__episode")).toEqual([]);

    // The whole point of M4.3: the user's own subtitle file is exactly where it was, byte for byte.
    expect(existsSync(userSubtitle)).toBe(true);
    expect(Buffer.compare(readFileSync(userSubtitle), readFileSync(sourceFixture))).toBe(0);
    // And the folder the user chose is still theirs.
    expect(existsSync(projectFolder)).toBe(true);
  });
});
