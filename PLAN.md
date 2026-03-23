# Sidebar thread grouping — worktree path canonicalization

## Problem

Threads in the sidebar are grouped by their `folder_paths` (a `PathList` stored
in the thread metadata database). When a thread is created from a git worktree
checkout (e.g. `/Users/eric/repo/worktrees/zed/lasalle-lceljoj7/zed`), its
`folder_paths` records the worktree path. But the sidebar computes workspace
groups from `visible_worktrees().abs_path()`, which returns the root repo path
(e.g. `/Users/eric/repo/zed`). Since `entries_for_path` did exact `PathList`
equality, threads from worktree checkouts were invisible in the sidebar.

## What we've done

### 1. `PathList` equality fix (PR #52052 — ready to merge)

**File:** `crates/util/src/path_list.rs`

`PathList` derived `PartialEq`/`Eq`/`Hash` which included the `order` field
(display ordering of paths). Two `PathList` values with the same paths in
different order were considered unequal. This caused thread matching to break
after worktree reordering in the project panel.

**Fix:** Manual `PartialEq`/`Eq`/`Hash` impls that only compare the sorted
`paths` field.

### 2. Worktree path canonicalization (on this branch, not yet PR'd)

**File:** `crates/sidebar/src/sidebar.rs`

Added two functions:
- `build_worktree_root_mapping()` — iterates all repo snapshots from all open
  workspaces and builds a `HashMap<PathBuf, Arc<Path>>` mapping every known
  worktree checkout path to its root repo path (using `original_repo_abs_path`
  and `linked_worktrees` from `RepositorySnapshot`).
- `canonicalize_path_list()` — maps each path in a `PathList` through the
  worktree root mapping, producing a canonical `PathList` keyed by root repo
  paths.

In `rebuild_contents`, instead of querying `entries_for_path(&path_list)` with
the workspace's literal path list, we now:
1. Build the worktree→root mapping once at the top
2. Iterate all thread entries and index them by their canonicalized `folder_paths`
3. Query that canonical index when populating each workspace's thread list

Also applied the same canonicalization to `find_current_workspace_for_path_list`
and `find_open_workspace_for_path_list` (used by archive thread restore).

**Status:** The core grouping works — threads from worktree checkouts now appear
under the root repo's sidebar header. But there are remaining issues with the
archive restore flow and workspace absorption.

## Remaining issues

### Archive thread restore doesn't route correctly

When restoring a thread from the archive, `activate_archived_thread` tries to
find a matching workspace via `find_current_workspace_for_path_list`. If the
thread's `folder_paths` is a single worktree path (e.g. `[zed/meteco/zed]`),
canonicalization maps it to `[/Users/eric/repo/zed]`. But if the current window
only has an `[ex, zed]` workspace, the canonical `[zed]` doesn't match `[ex,
zed]` — they're different path sets. So it falls through to
`open_workspace_and_activate_thread`, which opens the correct worktree but:
- The new workspace gets **absorbed** under the `ex, zed` header (no separate
  "zed" header appears)
- The thread activation may not route to the correct agent panel

This needs investigation into how absorption interacts with the restore flow,
and possibly the creation of a dedicated "zed" workspace (without ex) for
threads that were created in a zed-only context.

### Path set mutation (adding/removing folders)

When you add a folder to a project (e.g. adding `ex` to a `zed` workspace),
existing threads saved with `[zed]` don't match the new `[ex, zed]` path list.
Similarly, removing `ex` leaves threads saved with `[ex, zed]` orphaned.

This is a **design decision** the team is still discussing. Options include:
- Treat adding/removing a folder as mutating the project group (update all
  thread `folder_paths` to match)
- Show threads under the closest matching workspace
- Show "historical" groups for path lists that have threads but no open workspace

### Absorption suppresses workspace headers

When a worktree workspace is absorbed under a main repo workspace, it doesn't
get its own sidebar header. This is by design for the common case (you don't
want `zed` and `zed/meteor-36zvf3d7` as separate headers). But it means that a
thread from a single-path worktree workspace like `[zed/meteco/zed]` has no
header to appear under if the main workspace is `[ex, zed]` (different path
count).

## Key code locations

- **Thread metadata storage:** `crates/agent_ui/src/thread_metadata_store.rs`
  - `SidebarThreadMetadataStore` — in-memory cache + SQLite DB
  - `threads_by_paths: HashMap<PathList, Vec<ThreadMetadata>>` — index by literal paths
  - DB location: `~/Library/Application Support/Zed/db/0-{channel}/db.sqlite` table `sidebar_threads`
- **Old thread storage:** `crates/agent/src/db.rs`
  - `ThreadsDatabase` — the original thread DB (being migrated from)
  - DB location: `~/Library/Application Support/Zed/threads/threads.db`
- **Sidebar rebuild:** `crates/sidebar/src/sidebar.rs`
  - `rebuild_contents()` — the main function that assembles sidebar entries
  - `build_worktree_root_mapping()` — new: builds worktree→root path map
  - `canonicalize_path_list()` — new: maps a PathList through the root mapping
  - Absorption logic starts around "Identify absorbed workspaces"
  - Linked worktree query starts around "Load threads from linked git worktrees"
- **Thread saving:** `crates/agent/src/agent.rs`
  - `NativeAgent::save_thread()` — snapshots `folder_paths` from `project.visible_worktrees()` on every save
- **PathList:** `crates/util/src/path_list.rs`
  - Equality now compares only sorted paths, not display order
- **Archive restore:** `crates/sidebar/src/sidebar.rs`
  - `activate_archived_thread()` → `find_current_workspace_for_path_list()` → `open_workspace_and_activate_thread()`

## Useful debugging queries

```sql
-- All distinct folder_paths in the sidebar metadata store (nightly)
sqlite3 ~/Library/Application\ Support/Zed/db/0-nightly/db.sqlite \
  "SELECT folder_paths, COUNT(*) FROM sidebar_threads GROUP BY folder_paths ORDER BY COUNT(*) DESC"

-- All distinct folder_paths in the old thread store
sqlite3 ~/Library/Application\ Support/Zed/threads/threads.db \
  "SELECT folder_paths, COUNT(*) FROM threads WHERE parent_id IS NULL GROUP BY folder_paths ORDER BY COUNT(*) DESC"

-- Find a specific thread
sqlite3 ~/Library/Application\ Support/Zed/db/0-nightly/db.sqlite \
  "SELECT session_id, title, folder_paths FROM sidebar_threads WHERE title LIKE '%search term%'"

-- List all git worktrees for a repo
git -C /Users/eric/repo/zed worktree list --porcelain
```
