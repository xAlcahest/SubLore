# Pre-publication check, 2026-08-31

Run before making `xAlcahest/SubLore` public, against the whole git history rather than the working tree. Owner ruling 8a: if anything here needed history rewriting, the answer was to stop and report, not to rewrite.

**Nothing needs rewriting. The repository is safe to open.**

## What was checked, and how

**Secrets, over every commit.** `gitleaks` is not installed on this machine, so the scan was done directly: every commit reachable from every ref was grepped for GitHub tokens (`ghp_`, `github_pat_`), private key headers (RSA, OpenSSH, EC, DSA), OpenAI-style keys, AWS access key ids and Slack tokens. 569 blobs across 43 commits. **No match.** The working tree was scanned separately, with the same result.

**Binaries, models, media and generated artefacts.** No blob over 200 KB has ever been committed, in any commit. The video fixture, 3.8 MB, is **not** tracked: `fixtures/video/make-sample.sh` generates it, and CI runs that script (`ci.yml:119`) rather than carrying the file. The largest tracked files are `pnpm-lock.yaml` at 176 KB and a 2,000-line subtitle fixture at 132 KB. Total packed size of the tracked history is 507 KB — the 17 GB in the working directory is build output, all ignored.

**The owner's home path.** `/home/alcahest` appeared in `docs/reviews/gate-2-plan.md` (twice) and `docs/reviews/review-prompt.md` (once), both of which are working documents rather than historical reports, so both were rewritten to `<repo>`. It remains in `docs/reports/` and `docs/research/`, which the ruling tolerates: those are records of what was run on a particular machine on a particular day, and rewriting them would make them say something that did not happen. No script, config or source file carries an absolute path.

**Licence.** `LICENSE` is the GPL version 3 text; `src-tauri/Cargo.toml` declares `license = "GPL-3.0-or-later"`. Consistent with CLAUDE.md section 4, which puts the editor, playback, formats and the whisper integration in the open repository under GPL-3.0.

**Closed-module material.** No source, test or configuration references the pro modules. `decisions.md`, `BACKLOG.md`, `CLAUDE.md` and `post-v1-plan.md` discuss the open-core boundary as a design decision — which is the boundary being described, not code crossing it. No licence key, no key-generation tooling, no private-module path.

## What a reader will find, and should

The repository holds a pre-alpha with no releases, its full design record, and the record of a review gate that found two blockers in code written the same day and a third that one of the fixes created. That record is public on purpose: it is more useful to someone deciding whether to trust this project than a clean history would be.
