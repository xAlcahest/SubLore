/* global document, window */
/**
 * Every element the window edge cuts through, named.
 *
 * The regression fixture for this criterion is the `Save copy to` label clipped at 1024x700, so it
 * is asserted over the whole interface and not only over the layout's own boxes. Sideways is
 * absolute: nothing may cross the left or the right edge. Downwards is not, because a virtualized
 * list is taller than its viewport on purpose, so an element under the bottom edge only counts when
 * nothing above it scrolls.
 *
 * Written for `shell.spec.js`, which owns the two probes that prove the sweep can fail, and shared
 * with `interface-scale.spec.js`, which runs it at each end of the interface size range (S1).
 */
import { browser } from "@wdio/globals";

export function clippedAtWindowEdge(slop) {
  return browser.execute((allowed) => {
    const scrollsVertically = (element) => {
      for (let node = element.parentElement; node !== null; node = node.parentElement) {
        // Computed overflow, not scrollHeight: `overflow: hidden` overflows without scrolling, and
        // reading the height alone excused every box under the window edge.
        const overflow = window.getComputedStyle(node).overflowY;
        if (
          (overflow === "auto" || overflow === "scroll") &&
          node.scrollHeight > node.clientHeight
        ) {
          return true;
        }
      }
      return false;
    };
    const clipped = [];
    for (const element of document.querySelectorAll("*")) {
      const rect = element.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        continue;
      }
      const name = `${element.tagName.toLowerCase()}.${element.className}`;
      if (rect.left < -allowed || rect.right > window.innerWidth + allowed) {
        clipped.push(`${name} spans ${Math.round(rect.left)}..${Math.round(rect.right)} across`);
      } else if (rect.bottom > window.innerHeight + allowed && !scrollsVertically(element)) {
        clipped.push(`${name} ends ${Math.round(rect.bottom)} down`);
      }
    }
    return clipped;
  }, slop);
}
