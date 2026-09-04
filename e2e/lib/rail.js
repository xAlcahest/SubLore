/* global document, window */
/**
 * The rail's own gestures, shared because the state they establish is shared.
 *
 * Every spec in a run gets one data home, so the app reopens whichever project the spec before it
 * left open (decision 24, D5). A spec that reads the emptiest state has to make that state rather
 * than assume it, which is the rule M3.7's criteria state: establish what you need, do not inherit
 * it. `project.spec.js` had `closeAnyOpenProject` to itself and every other spec was exposed;
 * `command-registry.spec.js` was the one that noticed, by looking for a `.rail__empty` that a
 * project two specs earlier had replaced.
 */
import { browser } from "@wdio/globals";

import { appLogSinceStart, dataHome } from "./applog.js";
import { clickAt } from "./input.js";
import { waitFor } from "./proc.js";

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** Centre of an element in physical pixels, which is what X11 pointer coordinates are. */
async function centreOf(selector) {
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

/** The project node's own menu: project commands, and the one that adds an episode. */
export async function openProjectMenu(toplevel) {
  const open = await present(".rail__project");
  await clickElement(toplevel, open ? ".rail__project" : ".rail__empty");
  await waitFor(() => present(".railmenu"), {
    timeout: 15000,
    message: "the rail's project menu to open",
  });
}

export async function chooseRailItem(toplevel, key) {
  await clickElement(toplevel, `.railmenu__item--${key}`);
  await waitFor(async () => (await present(".railmenu")) === false, {
    timeout: 15000,
    message: `the menu to close after ${key}`,
  });
}

export async function confirmRailDialog(toplevel) {
  await clickElement(toplevel, ".raildialog__confirm");
  await waitFor(async () => (await present(".raildialog")) === false, {
    timeout: 15000,
    message: "the dialog to close after it was confirmed",
  });
}

/**
 * Whether this launch is about to reopen a project, waiting until the app knows.
 *
 * The rail cannot answer it. The restore is asynchronous, so an empty rail means either that there
 * is no project or that the one there is has not arrived yet, and those are not the same state.
 * The app says which as soon as it has read the remembered session, and that line is read from this
 * launch's own part of the shared log.
 */
async function willReopen() {
  const said = await waitFor(
    () =>
      /project session: read, (nothing to reopen|reopening )/.exec(appLogSinceStart(dataHome())),
    {
      timeout: 30000,
      message: "the app to say what it remembered of the last session",
    },
  );
  return said[1] !== "nothing to reopen";
}

/**
 * Start from nothing open, whoever the open project belonged to. Safe to call when the rail is
 * already empty, so a spec can put it in its `before` without asking first.
 *
 * The guard here used to be `.rail__project` in the DOM, which lost the race the whole of this file
 * exists to win: a `before` that ran before the restore painted returned having closed nothing, and
 * the project landed two tests later. See docs/restore-race.md in the meta repository.
 */
export async function closeAnyOpenProject(toplevel) {
  if (!(await willReopen())) {
    return;
  }
  await waitFor(() => present(".rail__project"), {
    timeout: 20000,
    message: "the project the app said it was reopening to reach the rail",
  });
  await openProjectMenu(toplevel);
  await chooseRailItem(toplevel, "close-project");
  await confirmRailDialog(toplevel);
  await waitFor(() => present(".rail__empty"), {
    timeout: 20000,
    message: "the rail to empty once another spec's project is closed",
  });
}
