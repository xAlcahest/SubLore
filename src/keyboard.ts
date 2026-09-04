/**
 * The one place that decides whether a key press belongs to the shell, and which command it asks
 * for. Before this the answer lived in three window listeners that could not see each other, and
 * eight commands had no key because adding one meant choosing which of the three to grow. See
 * docs/keyboard-tasks.md.
 */
import { isDocumentEditor } from "./components/cueView";
import { type CommandId, type CommandRegistry } from "./types/chrome";

/** Input types that hold typed text, and so keep their own undo. A range slider holds none. */
const TEXT_INPUT_TYPES = ["text", "search", "url", "email", "tel", "password", "number"];

/**
 * The chords a text field owns natively: its own undo and redo, its own selection, its own
 * clipboard. Everything outside this set belongs to the shell even with the caret in a field, or
 * Ctrl+F could not be pressed twice from the find band and Ctrl+S could not save while a search box
 * had focus, which is what every editor does. See docs/keyboard-tasks.md.
 */
const FIELD_CHORDS: ReadonlySet<string> = new Set(["a", "c", "v", "x", "y", "z"]);

/**
 * Whether this press belongs to the field it landed in rather than to the shell.
 *
 * Two conditions, and both have to hold. The target is a text field that is not one of the
 * document's own editors, because Ctrl+Z inside those is the document's undo and never the
 * webview's, which would fork the two histories. And the chord is one a text field actually owns.
 */
export function ownsTheKeyboard(target: EventTarget | null, key: string): boolean {
  if (!FIELD_CHORDS.has(key)) {
    return false;
  }
  if (!(target instanceof HTMLElement) || isDocumentEditor(target)) {
    return false;
  }
  if (target instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.includes(target.type);
  }
  return target instanceof HTMLTextAreaElement || target.isContentEditable;
}

/**
 * A declared accelerator, in the shapes the strings use: `Ctrl+O`, `Ctrl+Shift+S`, `Ctrl+1`.
 *
 * `on` is which property of the press the value is compared against, and the two are not
 * interchangeable. A letter is `key`, because on AZERTY Ctrl+A must be the key labelled A and that
 * key's `code` is `KeyQ`. A digit is `code`, because `key` carries the glyph the layout puts there:
 * measured, the same physical key reads `1` under `us`, `&` under `fr`, and `!` under Shift.
 */
type Chord = { shift: boolean; on: "key" | "code"; value: string };

/** Anything this cannot express returns null: the menu draws the string and no key fires it. */
function parseAccelerator(text: string | undefined): Chord | null {
  if (text === undefined) {
    return null;
  }
  const parts = text.split("+").map((part) => part.trim());
  const token = parts.pop();
  const modifiers = parts.map((part) => part.toLowerCase());
  // Ctrl is required and Alt is not a modifier any of these use: AltGr arrives as ctrl+alt and is
  // typing. Anything else in the string is one this cannot honour.
  if (token === undefined || !modifiers.includes("ctrl")) {
    return null;
  }
  if (modifiers.some((part) => part !== "ctrl" && part !== "shift")) {
    return null;
  }
  const shift = modifiers.includes("shift");
  if (/^[0-9]$/.test(token)) {
    return { shift, on: "code", value: `Digit${token}` };
  }
  if (/^[a-z]$/i.test(token)) {
    return { shift, on: "key", value: token.toLowerCase() };
  }
  return null;
}

/**
 * The command a key press asks for, read off the registry rather than off a list of letters, so a
 * command that declares a shortcut has one and the label cannot name a key that does nothing.
 */
export function commandFor(commands: CommandRegistry, event: KeyboardEvent): CommandId | null {
  if (!event.ctrlKey || event.altKey || event.metaKey) {
    return null;
  }
  const pressed = event.key.toLowerCase();
  for (const command of Object.values(commands)) {
    const chord = parseAccelerator(command.accelerator);
    if (chord === null || chord.shift !== event.shiftKey) {
      continue;
    }
    if (chord.on === "code" ? chord.value === event.code : chord.value === pressed) {
      return command.id;
    }
  }
  return null;
}
