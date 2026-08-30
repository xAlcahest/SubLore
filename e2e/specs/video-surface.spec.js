/* global describe, it, before, afterEach, document, window */
/**
 * N2: the native video surface can be hidden and shown again (BACKLOG NOW block, decision 2).
 *
 * The assertion is on the visible frame, never on a state flag. `show()` is called in exactly one
 * place today, inside `video_open`, and its own comment warns it must run before mpv builds its
 * output: hide-then-show on an open video had never been exercised, and a surface can report
 * `IsViewable` while showing nothing at all (docs/reports/n2-probe.md).
 *
 * Driven through the product's own path: `VideoStage` observes `.stage__surface` and reports its
 * rectangle, so collapsing that element sends an empty region (hide) and restoring it sends a real
 * one (show). This is exactly what decision 1 will do when a menu opens over the video.
 */
import { execFileSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

import { browser, expect } from "@wdio/globals";

import { clickAt, focusWindow } from "../lib/input.js";
import { requireFfmpeg, saturation } from "../lib/pixels.js";
import { requireVideoFixture, videoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/**
 * Average saturation. Measured on this fixture before the threshold was chosen, on this machine
 * (Fedora, Mesa software rendering, Xvfb): the empty stage reads 0.005 and the colour bars read
 * 42.6. Brightness does not separate them — the grey chrome already spans black to white, so both
 * states read about 200 on a luma range.
 *
 * Not measured on the CI runner. If that stack renders the bars less saturated the setup wait fails
 * there rather than passing wrongly, because the same threshold gates the precondition.
 */
const PICTURE = 5;

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

/** Collapse or restore the stage, which is what makes the region empty or real. */
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

/** Collapse or restore, and prove the DOM actually did it before waiting on the surface. */
async function stageCollapsed(collapsed) {
  const height = await setStageCollapsed(collapsed);
  if (collapsed ? height !== 0 : height <= 0) {
    throw new Error(
      `the stage did not ${collapsed ? "collapse" : "come back"}: height is ${height}. ` +
        `Nothing below this proves anything about the surface.`,
    );
  }
  return height;
}

/**
 * The one surface, re-read every time: its geometry moves with the layout, and a stale rectangle
 * would measure the wrong pixels. More than one large child means a leak, and saying so here keeps
 * the finding at its cause instead of at whichever assertion trips later.
 */
function surfaceWindow(toplevel) {
  const large = childWindows(toplevel.id).filter((child) => child.width > 50 && child.height > 50);
  if (large.length > 1) {
    throw new Error(
      `expected one native surface, found ${large.length} (${large.map((w) => w.id).join(", ")}): ` +
        `a show that created a window instead of mapping the old one leaks them.\n${rootTree()}`,
    );
  }
  return large[0] ?? null;
}

/** The surface as it is right now, never the copy captured when the suite started. */
function currentSurface(toplevel) {
  const found = surfaceWindow(toplevel);
  if (found === null) {
    throw new Error(`the native surface is gone.\n${rootTree()}`);
  }
  return found;
}

/**
 * The playback position in seconds, read from the slider rather than the clock text: the text is
 * `Math.floor`ed to whole seconds (VideoControls.tsx:6-11), so a restart shorter than a second is
 * invisible to it. The slider carries hundredths, and both are written from mpv's own `time-pos`.
 */
async function position() {
  const raw = await browser.execute(
    () => document.querySelector(".controls__slider")?.value ?? null,
  );
  if (raw === null) {
    // Without this the strongest assertion in the file degrades to `null === null`, which passes
    // while proving nothing at all.
    throw new Error(".controls__slider is missing: there is no playback position to read");
  }
  return Number(raw);
}

describe("video surface hide and show", () => {
  let toplevel = null;

  before(async () => {
    requireVideoFixture();
    requireFfmpeg();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(() => browser.execute(() => document.querySelector(".bar__input") !== null), {
      timeout: 30000,
      message: "the app UI to render",
    });

    // Open the fixture through the bar, the way a person does.
    const field = await centreOf(".bar__input");
    clickAt(toplevel.absX + field.x, toplevel.absY + field.y);
    execFileSync("xdotool", ["type", "--delay", "5", videoFixture], { timeout: 15000 });
    // The keystrokes are real X11 events: clicking Open before they have landed submits an empty
    // path, and the failure then arrives 30 s later blaming mpv for a race in the harness.
    await waitFor(
      () =>
        browser.execute(
          (want) => document.querySelector(".bar__input")?.value === want,
          videoFixture,
        ),
      { timeout: 15000, message: "the typed path to reach the field" },
    );
    const button = await centreOf(".bar__button");
    clickAt(toplevel.absX + button.x, toplevel.absY + button.y);

    // mpv attaching is the honest signal, not the map state: a surface with no mpv child reports
    // IsViewable while showing the webview underneath.
    const surface = await waitFor(
      () => {
        const found = surfaceWindow(toplevel);
        return found !== null && childWindows(found.id).length > 0 ? found : null;
      },
      { timeout: 30000, message: `the surface with mpv attached inside it.\n${rootTree()}` },
    );
    await waitFor(() => (saturation(surface) > PICTURE ? true : null), {
      timeout: 15000,
      message: "a picture on the surface before the tests begin",
    });
    // Clicking a disabled transport does nothing and the first test would then measure a video
    // that never started.
    await waitFor(
      () => browser.execute(() => document.querySelector(".controls__button")?.disabled === false),
      { timeout: 15000, message: "the transport button to become enabled" },
    );
  });

  afterEach(async () => {
    // A test that fails between collapse and restore would leave every later one measuring a
    // hidden surface and blaming the wrong thing.
    await setStageCollapsed(false);
  });

  it("brings the picture back after hide and show, with the video playing", async () => {
    const play = await centreOf(".controls__button");
    clickAt(toplevel.absX + play.x, toplevel.absY + play.y);

    // Playback started, proved by mpv's clock moving. Without this the click could have missed and
    // the rest of the test would run against a paused video while claiming otherwise.
    const started = await position();
    await waitFor(async () => ((await position()) > started ? true : null), {
      timeout: 15000,
      message: `the playback position to advance after Play (still ${started})`,
    });

    await stageCollapsed(true);
    await waitFor(() => (mapState(currentSurface(toplevel).id) === "IsUnMapped" ? true : null), {
      timeout: 10000,
      message: `the surface to hide when the region goes empty.\n${rootTree()}`,
    });
    // Unmapped and actually gone from the screen: the map state alone has already been wrong once.
    expect(saturation(currentSurface(toplevel))).toBeLessThan(PICTURE);

    await stageCollapsed(false);
    await waitFor(() => (saturation(currentSurface(toplevel)) > PICTURE ? true : null), {
      timeout: 15000,
      message: `the picture to come back after the region is restored.\n${rootTree()}`,
    });
    expect(mapState(currentSurface(toplevel).id)).toBe("IsViewable");

    // "playback continues", the second half of the AC: the clock is still moving afterwards.
    const afterShow = await position();
    await waitFor(async () => ((await position()) > afterShow ? true : null), {
      timeout: 15000,
      message: `playback to continue after the surface came back (stuck at ${afterShow})`,
    });
  });

  it("brings the picture back with the video paused, without restarting playback", async () => {
    const pause = await centreOf(".controls__button");
    clickAt(toplevel.absX + pause.x, toplevel.absY + pause.y);

    // Anchored to the value the app is supposed to show, not to itself: comparing the label with
    // its own earlier reading passes even when the click never landed and nothing was paused.
    await waitFor(
      async () =>
        (await browser.execute(
          () => document.querySelector(".controls__button")?.textContent ?? null,
        )) === "Play"
          ? true
          : null,
      { timeout: 10000, message: 'the transport button to read "Play", meaning paused' },
    );
    // mpv's own position, held still across a real interval. A `waitFor` here would return on its
    // first evaluation and compare a reading with itself: that is the defect the previous pass
    // blocked, and it came back inside its own correction.
    const frozen = await position();
    await sleep(1500);
    expect(await position()).toBe(frozen);

    await stageCollapsed(true);
    await waitFor(() => (mapState(currentSurface(toplevel).id) === "IsUnMapped" ? true : null), {
      timeout: 10000,
      message: "the surface to hide while paused",
    });

    await stageCollapsed(false);
    // Nothing else is sent: no seek, no play, no forced redraw. If the frame needs one of those to
    // come back, that is a finding to report, not something to hide inside the test.
    await waitFor(() => (saturation(currentSurface(toplevel)) > PICTURE ? true : null), {
      timeout: 15000,
      message: `the paused frame to come back with no nudge.\n${rootTree()}`,
    });

    // Still the same frame: the position never moved, so nothing restarted to redraw it. Checked
    // after a real interval, so a restart has room to show itself.
    await sleep(1500);
    expect(await position()).toBe(frozen);
  });

  it("survives ten hide and show cycles without leaking a surface", async () => {
    for (let cycle = 0; cycle < 10; cycle += 1) {
      await stageCollapsed(true);
      await waitFor(() => (mapState(currentSurface(toplevel).id) === "IsUnMapped" ? true : null), {
        timeout: 10000,
        message: `the surface to hide on cycle ${cycle}`,
      });
      await stageCollapsed(false);
      await waitFor(() => (mapState(currentSurface(toplevel).id) === "IsViewable" ? true : null), {
        timeout: 10000,
        message: `the surface to show on cycle ${cycle}`,
      });
    }

    // One surface, not eleven: a show that created a window instead of mapping the old one would
    // leave the extras behind.
    const remaining = childWindows(toplevel.id).filter((c) => c.width > 50 && c.height > 50);
    expect(remaining).toHaveLength(1);
    // The map is asynchronous on the X server: measuring immediately reads the frame before mpv
    // has repainted into it.
    await waitFor(() => (saturation(currentSurface(toplevel)) > PICTURE ? true : null), {
      timeout: 15000,
      message: "the picture to be alive after ten cycles",
    });
  });
});
