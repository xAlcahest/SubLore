/* global describe, it, before, document, window */
/**
 * Criteria 2, 3 and 4 of `module-abi.md` §9, through the app: a module file that is present and does
 * not load is a fault, and a fault is said out loud, once transiently in the status bar and once
 * permanently in About.
 *
 * The fixture is put beside the executable by `wdio.conf.js`, not here. The app is launched by the
 * session, before any hook in this file runs, so a fixture installed in a `before` would arrive
 * after the scan it is meant to be found by.
 *
 * Criterion 1, that an absent module is silence, is the other twenty-six spec files: every one of
 * them runs with no module beside the executable and none of them has ever drawn a module line.
 * Asserting it once more here would be asserting it about the same state they all establish.
 */
import { browser, expect } from "@wdio/globals";

import { appLog, dataHome } from "../lib/applog.js";
import { clickAt, focusWindow, pressKey } from "../lib/input.js";
import { windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** What the refused fixture beside the executable is, and what this build speaks. */
const FIXTURE_FILE = "sublore_module_wrong_major.so";
/** The good fixture's own title, rendered in the locale the host handed it. */
const MODULE_TITLE = "Fixture (en)";
/**
 * The fixture's item, with the answer the host gave it in the label.
 *
 * The fixture asks the host to find "the fog" in "the {\\i1}fog and the fog" with tags skipped.
 * Those bytes do not contain the term and what a reader sees does, so a two says the comparison
 * came from `sublore-matcher` through the host table and not from anything in the module.
 */
const MODULE_ITEM = "Say something (2 found)";
/** The fixture reports one above the host's major, and the host's is 1. */
const THEIRS = 2;
const OURS = 1;

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
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

describe("modules beside the executable", () => {
  let toplevel = null;

  before(async () => {
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__file-open-subtitle") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
  });

  it("puts the module's own title on the menu bar, with its item under it", async () => {
    // The core never learned this word. It came out of the module through describe, in the locale
    // the host handed it, and the title exists because a module pushed one (module-abi.md 5.1).
    const titles = await browser.execute(() =>
      Array.from(document.querySelectorAll(".menubar__title")).map(
        (title) => title.textContent ?? "",
      ),
    );
    expect(titles).toContain(MODULE_TITLE);

    const bar = await findToplevel();
    await clickElement(bar, `.menubar__title--module-0-1`);
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the module's own dropdown to open",
    });
    const items = await browser.execute(() =>
      Array.from(document.querySelectorAll(".menubar__item")).map((item) => item.textContent ?? ""),
    );
    expect(items.some((item) => item.includes(MODULE_ITEM))).toBe(true);
    // Exactly one. The fixture pushes a third item with a state this build does not know, and
    // section 5.2 says that costs the module its item rather than giving the user a control that
    // is enabled when it should not be.
    expect(items).toHaveLength(1);

    pressKey("Escape");
    await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
      timeout: 15000,
      message: "the dropdown to close",
    });
  });

  it("puts what the module logged in the app's own log, under the module's file name", () => {
    // Written by `describe`, through the host's own logger, and carrying the file it came from so
    // the line can be told from a core one (module-abi.md 4.2).
    expect(appLog(dataHome())).toContain(
      'module sublore_module_fixture.so: asked for "the fog" and was given 2 of them',
    );
  });

  it("says so in the status bar, naming the file and both versions", async () => {
    const line = await waitFor(async () => (await textOf(".statusbar__module-error")) ?? null, {
      timeout: 20000,
      message: "the status bar to report the module it could not use",
    });

    // The file name comes from the directory and the versions are integers, so the sentence is a
    // core string with data in it and the core never learned what the module was for.
    expect(line).toContain(FIXTURE_FILE);
    expect(line).toContain(String(THEIRS));
    expect(line).toContain(String(OURS));
  });

  it("says the same thing in About, where it stays", async () => {
    await clickElement(toplevel, ".menubar__title--help");
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the Help menu to open",
    });
    await clickElement(toplevel, "#menuitem-help-about");
    await waitFor(() => present(".about"), {
      timeout: 15000,
      message: "the About panel to open",
    });

    const drawn = await textOf(".about__module--refused");
    expect(drawn).toContain(FIXTURE_FILE);
    expect(drawn).toContain(String(THEIRS));
    // Nothing loaded, so the only module line About carries is the refusal.
    expect(await present(".about__modules-heading")).toBe(true);

    pressKey("Escape");
    await waitFor(async () => ((await present(".about")) === false ? true : null), {
      timeout: 15000,
      message: "the About panel to close",
    });
  });
});
