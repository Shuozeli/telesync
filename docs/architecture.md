# telesync Architecture

Last updated: 2026-04-22

## Overview

telesync is a CLI tool that syncs local markdown files to Telegraph pages. It uses a plan-and-execute pattern: build a diff between the filesystem and Telegraph state, then apply changes.

## Data Flow

```
markdown files
    |
    v
build_plan() --> compare files against SyncState
    |
    v
SyncPlan (Create/Update/Delete actions)
    |
    v
execute_plan() --> Telegraph API calls
    |
    v
SyncState saved to .telesync-state.json
```

## Module Structure

```
src/
  main.rs           -- clap CLI, command dispatch, error handling
  lib.rs            -- pub module re-exports
  client.rs         -- TelegraphClient (HTTP POST to Telegraph API)
  config.rs         -- Config / Publication / AccountConfig, TOML parsing + validation
  error.rs          -- TelesyncError enum
  markdown.rs       -- Frontmatter extraction, markdown->Telegraph nodes, content hashing, safety checks
  state.rs          -- SyncState (page tracking), Lockfile (pid-based concurrency lock)
  sync.rs           -- Plan building (build_plan) and execution (execute_plan)
  types.rs          -- Telegraph API types (Account, Page, Node, etc.)
```

## Plan Types

```rust
pub enum PlannedAction {
    Create { file, title, nodes, author_name, author_url },
    Update { file, path, title, nodes, author_name, author_url, new_hash },
    Delete { file, path },
    UpToDate { file },
    Skipped { file, reason },
    Failed { file, reason },
}
```

## State

```
.telesync-state.json
  pages: {
    "posts/my.md": {
      publication: "blog",
      telegraph_path: "My-Post-04-22-1234",
      telegraph_url: "https://telegra.ph/My-Post-04-22-1234",
      content_hash: "sha256:...",
      title: "My Post",
      last_synced: "2026-04-22T...",
      status: "published"
    }
  }
```

## Key Design Decisions

- **Lockfile**: Uses PID-based lock at `.telesync.lock` to prevent concurrent syncs. Stale locks are auto-cleaned.
- **Idempotent creates**: `build_plan` detects pages already published with same content hash, marks them `UpToDate` instead of recreating.
- **Retry with backoff**: `execute_create` and `execute_update` retry up to 3 times with exponential backoff on API errors.
- **Duplicate detection**: After a failed create attempt, checks Telegraph for an existing page with matching title before retrying.
- **Content hashing**: SHA256 of content + title + author_name (ignoring frontmatter whitespace differences).
- **Tombstone deletion**: Instead of deleting from Telegraph, replaces content with a "Removed by telesync" tombstone and marks status `Deleted`.
