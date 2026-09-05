import { Fragment } from "react";

import { commandToken, runCommand, type CommandId, type CommandRegistry } from "../types/chrome";

/** One button on the strip: a registry id, and the short word it is drawn with. */
export type WaveBarButton = { id: CommandId; short: string };

type WaveBarProps = {
  /** Groups drawn in order with a divider between them, exactly as the main toolbar draws its own. */
  groups: WaveBarButton[][];
  commands: CommandRegistry;
};

/**
 * The waveform panel's own strip of controls, under the wave and above the current line.
 *
 * Its buttons are registry commands like every other control in the shell, so each one greys with
 * the record the menu bar draws and each one runs through the same gate (interface-spec 2.3). It
 * carries its own class names rather than the toolbar's: the toolbar is one ordered strip in the
 * chrome and the checks that walk it must not pick these up as part of it.
 */
export default function WaveBar({ groups, commands }: WaveBarProps) {
  return (
    <div className="wavebar">
      {groups.map((group, index) => (
        <Fragment key={group.map((button) => button.id).join("-")}>
          {index > 0 && <span className="wavebar__divider" />}
          {group.map(({ id, short }) => {
            const command = commands[id];
            // An id with no record is a list naming a command that no longer exists; drawing
            // nothing is wrong either way, so it fails where it is written rather than here.
            if (command === undefined) {
              return null;
            }
            return (
              <button
                className={`wavebar__button wavebar__${commandToken(id)}`}
                key={id}
                type="button"
                title={command.label}
                aria-label={command.label}
                disabled={!command.enabled}
                aria-pressed={command.checked}
                onClick={() => runCommand(commands, id)}
              >
                {short}
              </button>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}
