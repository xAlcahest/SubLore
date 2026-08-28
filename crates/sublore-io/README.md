# sublore-io

Durable file writes for Sublore: replace a file atomically, and keep a backup of what was there
before. No dependencies, no subtitle knowledge, no UI strings. Implements CLAUDE.md §3 for
BACKLOG.md M1.4.

## Replacing a file

`write_atomic` and `save_with_backup` never open the destination for writing. The sequence is:

1. Resolve the path. A symlink is written _through_ (the link survives, its target gets the bytes);
   a destination that exists and is not a regular file is refused.
2. Create `.sublore-tmp-{pid}-{n}` in the destination's own directory with `create_new`.
3. On unix, copy the destination's mode onto the temp file, so a save cannot change who may read
   the user's file.
4. Write every byte, `sync_all`, close.
5. `fs::rename` over the destination — atomic on Linux and Windows.
6. On unix, `sync_all` the directory, so the rename itself survives a power cut.

At every instant the destination holds the old content in full or the new content in full. A
failure at any step removes the temp file and returns the original error; the destination is
untouched.

There is no cross-filesystem fallback and there must not be one. The temp file lives in the
destination's directory, so a rename across devices cannot happen, and a copy-based fallback would
not be atomic. An unwritable directory is an error the user can act on, never a degraded write.

## Backups

`save_with_backup` archives the current content **before** it writes, and aborts the save if the
archive fails. Layout, inside Sublore's own directory:

```
{root}/{name}-{fnv1a64 of the resolved path:016x}/{name}.{YYYYMMDD-HHMMSS}[-{n}].bak
```

UTC stamps, no colons (illegal in Windows file names). Same-second collisions get `-1` … `-99`,
each claimed with `create_new`. `BACKUP_CAP = 10` per source file.

Deletion rules, which exist because a bug here would cost the user data:

- Pruning runs only when a new backup has just been written, only inside the store root, and only
  on names this crate itself produces: `{source name}.` + exactly eight digits + `-` + exactly six
  digits + an optional `-` and one or two digits + `.bak`, and only on regular files. Everything
  else in that directory is left alone forever.
- Ordering is numeric, not lexicographic, so `-10` is newer than `-2`.
- Pruning is best effort. The new backup is already safe, so a file that cannot be deleted (another
  program holds it open) must never turn a good save into a failed one.
- There is no sweeper, no startup cleanup, no time-based expiry, and no other delete path.

## Crash injection

Debug builds only, same idiom as `src-tauri/src/crash/force.rs`. `SUBLORE_IO_FAULT` selects one
point — `after-backup`, `after-temp-created`, `during-write`, `after-write`, `after-sync`,
`after-rename` — and the process `abort()`s there: no unwinding, no flushing, the same brutality as
a real kill. An unknown value arms nothing. A release binary never reads the variable.

`tests/crash_injection.rs` re-runs itself as a child once per point per destination state, five
times each (60 processes, ~5 s), and reads what the child left on disk.

## Accepted limitations

- **An abort leaves its temp file behind.** Sweeping stale temp files would mean deleting files in
  the user's directory on a heuristic, which CLAUDE.md §3 does not permit. The name is reserved and
  obvious. "Stale temp file cleanup" is in the parking lot, as an explicit user action.
- **The directory fsync is unix only**; Windows cannot open a directory as a file.
- **Windows attributes are not copied** onto the temp file: a read-only destination blocks the
  rename anyway, and a read-only temp file could not be cleaned up afterwards.
- **A directory sync that fails after the rename is reported as an error** even though the new
  bytes are already in place. The old content is in the backup either way; a failing disk is worth
  saying out loud.
