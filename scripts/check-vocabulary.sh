#!/bin/bash
# N8f: forbidden vocabulary out of the open repository (CLAUDE.md §4). Run as CI runs it:
# scripts/check-vocabulary.sh
set -u
# report() sits at the end of a pipe and sets $fail: without lastpipe that stage runs in a
# subshell and the assignment is lost.
shopt -s lastpipe
cd "$(git rev-parse --show-toplevel)" || exit 1

self="scripts/check-vocabulary.sh"
fail=0

report() {
  # $1 = section title, stdin = grep output. Prints and marks the run failed only if non-empty.
  local hits
  hits=$(cat)
  if [ -n "$hits" ]; then
    printf '\n%s\n%s\n' "$1" "$hits"
    fail=1
  fi
}

# ---------------------------------------------------------------------------------------------
# Class 1: product vocabulary (sublore-meta docs/module-abi.md §7).
#
# Tracked files minus the three roadmap docs (root-level only: a nested README.md documents
# open-core code and stays in scope), the two lockfiles, and this script itself.
class1_files() {
  git ls-files -z -- \
    ':!BACKLOG.md' ':!CONTRIBUTING.md' ':!README.md' \
    ':!pnpm-lock.yaml' ':!Cargo.lock' \
    ":!$self"
}

# Three shapes one grep needs to catch: standalone/snake_case, an inner camelCase segment, and a
# leading lowercase segment before an Uppercase+lowercase pair (so "PROMPT" is one word, not a hit).
word_hits() {
  local lower=$1 camel=$2
  local standalone="(?<![A-Za-z])(?i:${lower})(?![A-Za-z])"
  local inner="(?<=[a-z])${camel}(?=[A-Z]|[^A-Za-z]|\$)"
  local leading="(?<![A-Za-z])(?i:${lower})(?=[A-Z][a-z])"
  class1_files | xargs -0 -r grep -InP -- "${standalone}|${inner}|${leading}" 2>/dev/null
}

# termbase, glossary: not ordinary English words, no legitimate collision to guard against.
word_hits termbase Termbase | report "termbase:"
word_hits glossary Glossary | report "glossary:"

# QA, TM: acronyms, same three shapes; case-sensitive costs nothing since neither collides with
# real prose here either way.
word_hits qa QA | report "QA:"
word_hits tm TM | report "TM:"

# pro: the one word here with real English collisions (project, process, property, produce,
# provide); the three shapes above are exactly what keeps every one of them out.
word_hits pro Pro | report "pro:"

# translation memory: Sublore's own tagline is this exact phrase (LICENSE, Cargo.toml,
# src/i18n/en.ts's About screen), so only "translation memory for subtitles" is allowed; any
# other occurrence, including an identifier spelling, still fails.
class1_files | xargs -0 -r grep -InE '\btranslation[ _]?memory\b' 2>/dev/null |
  grep -viE 'translation memory for subtitles' |
  report "translation memory:"

# term: left out on purpose. \bterm\b also matches ordinary English, including GPL-3.0's own
# boilerplate (LICENSE:189,355,424-425); nothing here can tell a leak from prose.

# ---------------------------------------------------------------------------------------------
# Class 2: the reference editor's name never appears here, including in this script: decoded at
# run time from base64 so grepping this file for the name itself still finds nothing.
name=$(printf 'QWVnaXN1Yg==' | base64 -d) || { echo "check-vocabulary: base64 decode failed" >&2; exit 1; }
[ -n "$name" ] || { echo "check-vocabulary: decoded name is empty" >&2; exit 1; }

# The two fixtures that keep it: real bytes that editor produces, kept to prove Sublore
# round-trips an unknown ASS section losslessly.
git ls-files -z | xargs -0 -r grep -InFi -- "$name" 2>/dev/null |
  grep -vE '^fixtures/subtitles/ass/clean/(basic|unknown-sections)\.ass:' |
  report "reference editor's name:"

if [ "$fail" -ne 0 ]; then
  echo
  echo "check-vocabulary: forbidden text found (see above)"
  exit 1
fi

echo "check-vocabulary: clean"
