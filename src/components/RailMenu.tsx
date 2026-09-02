import { useEffect, useLayoutEffect, useRef, useState } from "react";

export type RailMenuItem = {
  /** Stable across renders, and the class the check names: `railmenu__item--<key>`. */
  key: string;
  label: string;
  run: () => void;
};

type RailMenuProps = {
  x: number;
  y: number;
  label: string;
  items: RailMenuItem[];
  onClose: () => void;
};

/**
 * The rail's context menu. A layer in the sense `shell-layout.md` gives the word: it is painted
 * over the panels, it is transient, and it is dismissed rather than resized away.
 *
 * Episode-level commands live here rather than in the menu bar (decision 24, A3).
 */
export default function RailMenu({ x, y, label, items, onClose }: RailMenuProps) {
  const list = useRef<HTMLUListElement>(null);
  const [at, setAt] = useState({ x, y });

  // Opened from a pointer or from the keyboard, and either way the first item is where the
  // keyboard starts.
  useEffect(() => {
    list.current?.querySelector("button")?.focus();
  }, []);

  // A menu opened near an edge would hang off the window, and a command nobody can reach is a
  // command that is not there. Measured once it is laid out, and moved before it is painted.
  useLayoutEffect(() => {
    const box = list.current?.getBoundingClientRect();
    if (box === undefined) {
      return;
    }
    setAt({
      x: Math.max(0, Math.min(x, window.innerWidth - box.width)),
      y: Math.max(0, Math.min(y, window.innerHeight - box.height)),
    });
  }, [x, y]);

  function walk(event: React.KeyboardEvent<HTMLUListElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") {
      return;
    }
    event.preventDefault();
    const buttons = Array.from(list.current?.querySelectorAll("button") ?? []);
    const here = buttons.findIndex((button) => button === document.activeElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    const next = (here + step + buttons.length) % buttons.length;
    buttons[next]?.focus();
  }

  return (
    <>
      {/* Anywhere else dismisses it, and swallows the click that dismissed it. */}
      <div className="railmenu__backdrop" onMouseDown={onClose} />
      <ul
        ref={list}
        className="railmenu"
        role="menu"
        aria-label={label}
        style={{ left: `${at.x}px`, top: `${at.y}px` }}
        onKeyDown={walk}
      >
        {items.map((item) => (
          <li key={item.key} role="none">
            <button
              className={`railmenu__item railmenu__item--${item.key}`}
              type="button"
              role="menuitem"
              onClick={() => {
                onClose();
                item.run();
              }}
            >
              {item.label}
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}
