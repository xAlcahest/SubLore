import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // .whisper/ is the whisper.cpp checkout and its build trees, which carry their own JS and
  // TypeScript. It is git-ignored; the linter has to be told separately. See BACKLOG.md M3.1.
  // Build output and local tooling. `.claude/worktrees/` holds agent worktrees, which are whole
  // copies of this repository: linting them finds every file twice and breaks the TypeScript
  // parser, which cannot pick a root among four candidates. The same list is in `.prettierignore`.
  { ignores: ["dist/", "target/", ".whisper/", ".claude/", ".omc/", "ci-logs/"] },
  js.configs.recommended,
  tseslint.configs.recommended,
);
