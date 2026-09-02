/**
 * One command of the shell chrome. The menu bar and the toolbar draw the same records, so a command
 * that reaches one route reaches the other by construction. See docs T3.
 */
export type Command = {
  /** Also the class suffix both routes carry: `toolbar__save` and `menubar__item--save`. */
  id: string;
  label: string;
  /** Drawn beside the menu item. The key itself is handled by whoever owns the command. */
  accelerator?: string;
  enabled: boolean;
  run: () => void;
};

/** A menu bar title and what it opens. A title with no items is never drawn (decision 24 A2). */
export type Menu = {
  id: string;
  title: string;
  items: Command[];
};
