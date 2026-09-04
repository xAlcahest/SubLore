import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** What a module put in the chrome. A shape and a label, and nothing that says what it does. */
export type Contribution = {
  /** Which module it came from. Sent back when the item is activated. */
  module: number;
  /** The module's own id, meaningless here and echoed back to it. */
  id: number;
  kind: "menuTitle" | "menuItem" | "separator" | "toolbarButton" | "panel";
  /** The title or panel it hangs off, or null for top level. */
  parent: number | null;
  enableWhen: "always" | "documentOpen" | "projectOpen" | "selectionNonEmpty";
  /** A panel that covers the video, so it registers as a layer (module-abi.md 5.4). */
  layer: boolean;
  label: string;
};

/**
 * What the loaded modules contribute, asked for once.
 *
 * The backend already called every module's `describe` before the window existed, so this reads a
 * result. An empty list is the ordinary case and the one with no module installed: nothing here
 * distinguishes "no modules" from "modules that contributed nothing", because the chrome draws the
 * same thing either way.
 */
export function useContributions(): Contribution[] {
  const [items, setItems] = useState<Contribution[]>([]);

  useEffect(() => {
    let listening = true;
    void invoke<Contribution[]>("module_contributions").then(
      (found) => {
        if (listening) {
          setItems(found);
        }
      },
      (error) => {
        console.error("module contributions: they could not be read", error);
      },
    );
    return () => {
      listening = false;
    };
  }, []);

  return items;
}
