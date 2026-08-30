#!/bin/sh
# Sublore SessionStart hook: pin the git identity to the owner (owner ruling 2026-08-30).
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
