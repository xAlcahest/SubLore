import { Fragment } from "react";

import { type Command } from "../types/chrome";

type ToolbarProps = {
  /** Groups drawn in order with a divider between them, as the layout draws the toolbar. */
  groups: Command[][];
};

/** The toolbar: the same command records the menu draws, so neither route can grow the other one. */
export default function Toolbar({ groups }: ToolbarProps) {
  return (
    <div className="toolbar">
      {groups.map((group, index) => (
        <Fragment key={group.map((command) => command.id).join("-")}>
          {index > 0 && <span className="toolbar__divider" />}
          {group.map((command) => (
            <button
              className={`toolbar__button toolbar__${command.id}`}
              key={command.id}
              type="button"
              disabled={!command.enabled}
              onClick={command.run}
            >
              {command.label}
            </button>
          ))}
        </Fragment>
      ))}
    </div>
  );
}
