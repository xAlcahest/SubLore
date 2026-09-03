/* global describe, it, before, document, window */
/**
 * The other half of the surface state machine (BACKLOG N2, second review pass).
 *
 * `apply_region` takes two inputs — whether the rectangle is empty, and whether a video is open —
 * and `video-surface.spec.js` only ever exercises the combinations that follow a successful open.
 * This file covers the rest, because the defect the first review pass blocked lived exactly there:
 * the surface was shown for any non-empty rectangle, so an opaque slab covered the empty stage from
 * the first layout onwards. Without these checks, a later simplification of `apply_region` brings
 * it back with a fully green suite.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/**
 * A file that exists and is not a video. The chooser only hands back paths that are really there,
 * so the failed open this file needs is one mpv refuses to decode rather than one that is missing.
 */
function brokenVideo() {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "video-empty");
  mkdirSync(directory, { recursive: true });
  const file = path.join(directory, "not-a-video.mkv");
  writeFileSync(file, "Not a Matroska file, and mpv says so.\n");
  return file;
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

/** The surface exists from startup, sized over the stage; only its map state changes. */
function surfaceWindow(toplevel) {
  return (
    childWindows(toplevel.id)
      .filter((child) => child.width > 50 && child.height > 50)
      .sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null
  );
}

function setStageCollapsed(collapsed) {
  return browser.execute((hide) => {
    const element = document.querySelector(".stage__surface");
    if (element === null) {
      throw new Error(".stage__surface is missing from the DOM");
    }
    element.style.height = hide ? "0px" : "";
    return element.getBoundingClientRect().height;
  }, collapsed);
}

describe("video surface with no video open", () => {
  let toplevel = null;

  before(async () => {
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__video-open") !== null),
      {
        timeout: 30000,
        message: "the app UI to render",
      },
    );
  });

  it("leaves the stage empty and the surface unmapped before anything is opened", async () => {
    // The frontend has laid out and sent at least one region by now, which is the moment the
    // blocked defect used to show the slab.
    const empty = await browser.execute(
      () => document.querySelector(".stage__empty")?.textContent ?? null,
    );
    expect(empty).not.toBe(null);

    const surface = surfaceWindow(toplevel);
    if (surface === null) {
      throw new Error(`no surface among the toplevel's children\n${rootTree()}`);
    }
    expect(mapState(surface.id)).toBe("IsUnMapped");
  });

  it("keeps the surface unmapped when the layout changes with no video", async () => {
    const surface = surfaceWindow(toplevel);
    await setStageCollapsed(true);
    await setStageCollapsed(false);
    // A real region arrives on the way back. Visibility must follow the video, not the rectangle.
    await waitFor(() => (mapState(surface.id) === "IsUnMapped" ? true : null), {
      timeout: 5000,
      message: "the surface to stay hidden through a layout change",
    }).catch(() => {
      throw new Error(
        `the surface was mapped by a layout change with no video open: the empty stage is covered ` +
          `by an opaque slab again.\n${rootTree()}`,
      );
    });
    expect(mapState(surface.id)).toBe("IsUnMapped");
  });

  it("keeps the surface unmapped after an open that failed", async () => {
    const button = await centreOf(".toolbar__video-open");
    clickAt(toplevel.absX + button.x, toplevel.absY + button.y);
    const chooser = await waitForChooser("Choose a video");
    await answerChooser(chooser, brokenVideo(), "video");
    focusWindow(toplevel.id);

    await waitFor(
      () =>
        browser.execute(
          () => document.querySelector(".statusbar__video-error")?.textContent ?? null,
        ),
      { timeout: 20000, message: "the open to fail with a message" },
    );

    const surface = surfaceWindow(toplevel);
    expect(mapState(surface.id)).toBe("IsUnMapped");

    // The failure path hides the surface; the danger is the next layout showing it again.
    await setStageCollapsed(true);
    await setStageCollapsed(false);
    await browser.pause(1000);
    expect(mapState(surface.id)).toBe("IsUnMapped");
  });
});
