/**
 * Placeholder filling for the source strings in en.ts. Exists so no user-facing sentence is ever
 * built by concatenation: a translator needs the whole sentence, word order included.
 */

/** Replace every `{name}` in `template`. An unknown name is left as written, so a bug shows. */
export function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (placeholder: string, key: string) =>
    Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : placeholder,
  );
}
