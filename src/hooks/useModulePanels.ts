import { useCallback, useState } from "react";

/**
 * One cell of one panel row, as the module pushed it.
 *
 * The kind says which of the two values is drawn, and the core has no other rule: text and badge
 * draw `text`, number draws `number`, percent draws `number` through a format. It never asks what
 * a cell means (module-abi.md 5.3).
 */
export type PanelCell = {
  kind: "text" | "number" | "percent" | "badge";
  text: string;
  number: number;
};

/**
 * One row, and the module's own handle for it.
 *
 * A string, because the handle is a `u64` and one above 2^53 does not survive a JSON number. It is
 * carried back to the module unread: nothing here interprets a handle.
 */
export type PanelRow = { handle: string; cells: PanelCell[] };

/** One panel a module filled, keyed by the module it came from and its own id. */
export type PublishedPanel = { module: number; panelId: number; rows: PanelRow[] };

/** The one key a panel is held under. Two modules may both use id 1, and they are not the same. */
function keyOf(module: number, panelId: number): string {
  return `${module}-${panelId}`;
}

export type ModulePanels = {
  /** What is on screen now, in the order the panels were first published. */
  panels: PublishedPanel[];
  /**
   * Take what one activation published.
   *
   * A publish with no rows clears that panel rather than leaving the last table up: a module saying
   * it has nothing to show is saying something. A panel this call did not publish is untouched,
   * because only the module that fills a panel knows when its contents have stopped being true.
   */
  publish: (published: PublishedPanel[]) => void;
  /** The user closed one. */
  close: (module: number, panelId: number) => void;
};

export function useModulePanels(): ModulePanels {
  const [panels, setPanels] = useState<PublishedPanel[]>([]);

  const publish = useCallback((published: PublishedPanel[]) => {
    if (published.length === 0) {
      return;
    }
    setPanels((current) => {
      const next = [...current];
      for (const panel of published) {
        const at = next.findIndex(
          (held) => keyOf(held.module, held.panelId) === keyOf(panel.module, panel.panelId),
        );
        if (panel.rows.length === 0) {
          if (at >= 0) {
            next.splice(at, 1);
          }
          continue;
        }
        if (at >= 0) {
          next[at] = panel;
        } else {
          next.push(panel);
        }
      }
      return next;
    });
  }, []);

  const close = useCallback((module: number, panelId: number) => {
    setPanels((current) =>
      current.filter((held) => keyOf(held.module, held.panelId) !== keyOf(module, panelId)),
    );
  }, []);

  return { panels, publish, close };
}
