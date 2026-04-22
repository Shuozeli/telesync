# telesync

Sync markdown files to [Telegraph](https://telegra.ph/) pages.

## Overview

telesync watches a directory of markdown files and keeps a set of Telegraph pages in sync. It handles create, update, and delete (tombstone) operations based on file changes, content hashes, and state tracking.

## Setup

1. Create a `telesync.toml` config file:

```toml
[accounts.my_blog]
short_name = "My Blog"
author_name = "Your Name"
author_url = "https://example.com"
access_token = "your-telegraph-access-token"

[[publications]]
name = "blog"
path = "posts"
account = "my_blog"
```

2. Create markdown files with frontmatter:

```markdown
---
title: My Post Title
author_name: Custom Author  # optional override
---

Your content here.
```

3. Run a sync:

```bash
cargo run -- sync
```

## Commands

```bash
# Sync all publications
cargo run -- sync

# Sync a specific publication only
cargo run -- sync --publication blog

# Dry run (show planned actions without making changes)
cargo run -- sync --dry-run

# Force re-sync a specific file (even if hash matches)
cargo run -- sync --force posts/my-post.md

# Confirm first-time sync (required for initial publish)
cargo run -- sync --confirm

# Create Telegraph accounts for entries missing tokens
cargo run -- accounts create
```

## Configuration

See `telesync.toml` for the full configuration schema. Key settings:

- `publications[].path` — directory to scan for `.md` files
- `publications[].account` — which account to publish under
- `accounts[].access_token` — Telegraph access token (get via `accounts create`)

## Features

- **Idempotent**: Same content produces same page, no duplicate pages
- **Content hashing**: Only updates when content actually changes
- **Frontmatter parsing**: Title, author_name, author_url from YAML frontmatter
- **Title inference**: Falls back to first heading or filename if no frontmatter title
- **Tombstone deletion**: Deleted files get a "Removed by telesync" tombstone page
- **Content safety**: Blocks files containing API keys or secrets
- **Lockfile**: Prevents concurrent sync operations
