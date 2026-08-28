import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // .whisper/ is the whisper.cpp checkout and its build trees, which carry their own JS and
  // TypeScript. It is git-ignored; the linter has to be told separately. See BACKLOG.md M3.1.
  { ignores: ["dist/", "target/", ".whisper/"] },
  js.configs.recommended,
  tseslint.configs.recommended,
);
