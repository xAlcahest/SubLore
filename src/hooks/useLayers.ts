import { createContext, useContext, useEffect, useId, useMemo, useState } from "react";

/**
 * The set of open HTML layers. A layer is anything the shell paints outside the panel flow — a
 * dropdown, a context menu, a dialog — and while any one is open the native video surface hides,
 * whether or not it overlaps the video rectangle (decision 1, T8). No geometry test: the surface is
 * an X11 child that stacks above the webview by construction, and a rule comparing rectangles would
 * have to be re-evaluated on every layout change.
 *
 * A set of ids rather than a counter or a flag, because a dialog can outlive the menu that opened
 * it, so removal is by id. A layer opening in the same commit as another closes leaves the derived
 * boolean untouched, so no frame reaches the screen in between.
 */
export type LayerRegistrar = {
  open: (id: string) => void;
  close: (id: string) => void;
};

/** Null outside the shell, which is a wiring mistake rather than a state a layer can open in. */
export const LayerContext = createContext<LayerRegistrar | null>(null);

export type LayerRegistry = {
  /** At least one layer is open, so the surface has nothing it may draw over. */
  covered: boolean;
  registrar: LayerRegistrar;
};

const NO_LAYERS: ReadonlySet<string> = new Set();

/**
 * The shell's own state, and the one owner of it. Surface visibility is derived from `covered`;
 * nothing here calls a video command.
 */
export function useLayerRegistry(): LayerRegistry {
  const [openIds, setOpenIds] = useState<ReadonlySet<string>>(NO_LAYERS);

  // Stable for the shell's life: every layer's effect depends on it, and a new object each render
  // would unregister and re-register every open layer.
  const registrar = useMemo<LayerRegistrar>(
    () => ({
      open: (id) => setOpenIds((current) => (current.has(id) ? current : new Set(current).add(id))),
      close: (id) =>
        setOpenIds((current) => {
          if (!current.has(id)) {
            return current;
          }
          const next = new Set(current);
          next.delete(id);
          return next;
        }),
    }),
    [],
  );

  return { covered: openIds.size > 0, registrar };
}

/**
 * Register this layer with the shell while `open`, and drop it on unmount. Mounting is the whole
 * contract, so a layer added later cannot forget to hide the picture (T8).
 */
export function useLayer(open: boolean): void {
  const registrar = useContext(LayerContext);
  // React's own id, so two layers of the same kind on screen at once cannot share a registration.
  const id = useId();

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    // A layer outside the provider would leave the video covering it, silently. See App.tsx.
    if (registrar === null) {
      throw new Error("useLayer was called outside the shell's LayerContext");
    }
    registrar.open(id);
    return () => registrar.close(id);
  }, [open, id, registrar]);
}
