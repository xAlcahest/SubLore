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
 * clipboard. A chord outside this set belongs to the shell even with the caret in a field, or
 * Ctrl+F could not be pressed twice from the find band and Ctrl+S could not save while a search box
 * had focus, which is what every editor does. See docs/keyboard-tasks.md.
 */
const FIELD_CHORDS: ReadonlySet<string> = new Set(["a", "c", "v", "x", "y", "z"]);

/**
 * F1 to F12, the only bare keys a text field has no use for.
 *
 * One definition, read twice: against an accelerator's token, and against a press's own `key`. The
 * two cannot be confused, because a function key's `key` is `F3` and the letter F's is `f`.
 */
const FUNCTION_KEY = /^f([1-9]|1[0-2])$/i;

/** Whether this element holds typed text. A checkbox, a slider and a button hold none. */
function isTextField(target: EventTarget | null): target is HTMLElement {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.includes(target.type);
  }
  return target instanceof HTMLTextAreaElement || target.isContentEditable;
}

/**
 * Whether this press belongs to the field it landed in rather than to the shell.
 *
 * Two answers, and which one is given turns on whether a modifier was held.
 *
 * With no modifier a text field owns every key but a function key. The allowlist is written that
 * way round rather than as a test for printability, because Backspace, Delete, Enter, Tab, Home,
 * End and the arrows all mean something inside a field and not one of them is a character. F1 to
 * F12 are what is left over, which is why every editor puts its shell commands there and why F3 can
 * step the search on from inside the band's own query field and from inside a cue being edited.
 * The half exists because `parseAccelerator` now takes a bare token at all: a future accelerator on
 * a bare `n` must not fire while someone types `n` into a cue.
 *
 * A chord keeps the older answer, the set above inside a field that is not one of the document's
 * editors, because Ctrl+Z in those is the document's undo and never the webview's, which would fork
 * the two histories. See docs/keyboard-tasks.md and F5.
 */
export function ownsTheKeyboard(
  target: EventTarget | null,
  key: string,
  chorded: boolean,
): boolean {
  if (!isTextField(target)) {
    return false;
  }
  if (!chorded) {
    return !FUNCTION_KEY.test(key);
  }
  return FIELD_CHORDS.has(key) && !isDocumentEditor(target);
}

/**
 * A declared accelerator, in the shapes the strings use: `Ctrl+O`, `Ctrl+Shift+S`, `Ctrl+1`, `F3`.
 *
 * `on` is which property of the press the value is compared against, and the two are not
 * interchangeable. A letter is `key`, because on AZERTY Ctrl+A must be the key labelled A and that
 * key's `code` is `KeyQ`. A digit is `code`, because `key` carries the glyph the layout puts there:
 * measured, the same physical key reads `1` under `us`, `&` under `fr`, and `!` under Shift. A
 * function key is `code` as well: it is one physical key with no glyph to shift into (F5).
 */
type Chord = { ctrl: boolean; shift: boolean; on: "key" | "code"; value: string };

/** Anything this cannot express returns null: the menu draws the string and no key fires it. */
function parseAccelerator(text: string | undefined): Chord | null {
  if (text === undefined) {
    return null;
  }
  const parts = text.split("+").map((part) => part.trim());
  const token = parts.pop();
  const modifiers = parts.map((part) => part.toLowerCase());
  // Alt is not a modifier any of these use: AltGr arrives as ctrl+alt and is typing. Anything else
  // in the string is one this cannot honour. Ctrl is no longer required: F3 has no modifier at all.
  if (token === undefined || modifiers.some((part) => part !== "ctrl" && part !== "shift")) {
    return null;
  }
  const ctrl = modifiers.includes("ctrl");
  const shift = modifiers.includes("shift");
  if (/^[0-9]$/.test(token)) {
    return { ctrl, shift, on: "code", value: `Digit${token}` };
  }
  // The function keys, whose `code` is the name they are drawn with (F5).
  const functionKey = FUNCTION_KEY.exec(token);
  if (functionKey !== null) {
    return { ctrl, shift, on: "code", value: `F${functionKey[1]}` };
  }
  if (/^[a-z]$/i.test(token)) {
    return { ctrl, shift, on: "key", value: token.toLowerCase() };
  }
  return null;
}

/**
 * The command a key press asks for, read off the registry rather than off a list of letters, so a
 * command that declares a shortcut has one and the label cannot name a key that does nothing.
 */
export function commandFor(commands: CommandRegistry, event: KeyboardEvent): CommandId | null {
  if (event.altKey || event.metaKey) {
    return null;
  }
  const pressed = event.key.toLowerCase();
  for (const command of Object.values(commands)) {
    const chord = parseAccelerator(command.accelerator);
    if (chord === null || chord.ctrl !== event.ctrlKey || chord.shift !== event.shiftKey) {
      continue;
    }
    if (chord.on === "code" ? chord.value === event.code : chord.value === pressed) {
      return command.id;
    }
  }
  return null;
}
