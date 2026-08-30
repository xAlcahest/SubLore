#!/bin/sh
# Sublore SessionStart hook: pin the git identity to the owner, and restate the
# standing rules an agent session cannot infer from the repo (owner rulings 2026-08-30).
#
# Claude Code on the web starts every container with a global git config of
# user.name=Claude / user.email=noreply@anthropic.com, and re-asserts it at each
# session start. An agent's first commit lands under that author unless something
# overrides it here; repository-local config wins over global, so this does.
#
# Signing is turned off on purpose: the container's signing key belongs to the agent
# identity, and a commit authored by the owner must never carry someone else's
# signature. Commits made from the web are unsigned; commits made on the owner's
# machine keep the signature they already had.
set -e

repo_root=${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}
git -C "$repo_root" config user.name "Alcahest"
git -C "$repo_root" config user.email "xaris@gzgd.info"
git -C "$repo_root" config commit.gpgsign false

# Printed, not just applied: hook stdout reaches the session, and a rule the agent
# never reads is a rule that gets broken once per session.
cat <<'RULES'
Sublore — standing rules for agent sessions (owner rulings 2026-08-30):
- Commits are authored by Alcahest <xaris@gzgd.info>. This hook has already set it;
  never set it back to an agent identity, and never sign from a web container.
- Branches are never named claude/* or after the agent. Use a topic name:
  ci/..., fix/..., feat/..., docs/....
RULES
