/* global window */
/**
 * Counting what crossed the IPC boundary, from inside the page.
 *
 * Every subtitle command travels on `fetch`, so a probe on `fetch` is how a spec says "one command
 * went" and how it says "none did". It grew inside `current-line.spec.js`, which needed the first;
 * `command-registry.spec.js` needs the second from three routes at once. One copy, because two
 * probes wrapping one `window.fetch` would each restore the other's wrapper.
 */
import { browser } from "@wdio/globals";

/**
 * Watch every subtitle command that crosses from here until `takeCommands`, in order. A second
 * route into the document shows up here as a second name.
 */
export function watchCommands() {
  return browser.execute(() => {
    window.__subloreCommands = [];
    const passThrough = window.fetch;
    window.__subloreFetch = passThrough;
    window.fetch = (...rest) => {
      const url = String(rest[0]?.url ?? rest[0]);
      const name = url.split("/").pop();
      if (typeof name === "string" && name.startsWith("subtitle_")) {
        window.__subloreCommands.push(name);
      }
      return passThrough.apply(window, rest);
    };
    if (window.fetch === passThrough) {
      throw new Error("the probe did not take: fetch is not writable here either");
    }
  });
}

/** The names seen since `watchCommands`, with the page's own `fetch` put back. */
export function takeCommands() {
  return browser.execute(() => {
    window.fetch = window.__subloreFetch;
    return window.__subloreCommands;
  });
}
