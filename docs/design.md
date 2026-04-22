# telesync Design

Last updated: 2026-04-22

## Problem

Publishing markdown to Telegraph is manual. Creating pages, updating after edits, and cleaning up deleted posts all require API calls or manual copy-pasting. Existing tools don't track local state, so it's hard to know what's published and what changed.

## Solution

telesync acts as a git-to-Telegraph sync engine. You manage local markdown files; telesync manages Telegraph pages. It detects changes via content hashing and applies the minimum set of operations (create/update/delete) on each sync.

## Key Design Choices

### State File Over API Discovery

Instead of querying Telegraph for all pages on each sync (which is slow and rate-limited), telesync maintains a local state file (`.telesync-state.json`). This file tracks which files map to which Telegraph paths and their content hashes.

### Content Hash = Idempotency Key

The content hash is `SHA256(content + title + author_name)`. This means:
- Same content always produces the same hash → no duplicate pages on re-sync
- Title or author changes trigger a new hash → update is detected
- Whitespace-only frontmatter changes are ignored (normalized before hashing)

### Tombstone Pattern for Deletes

Telegraph pages cannot be deleted via API — only edited. Instead of losing the page, telesync replaces the content with a "Removed by telesync on {date}" tombstone. The local state marks it `Deleted` so it's not re-created on future syncs.

### Confirmation for First Publish

Telegraph page paths include a random suffix (e.g., `My-Post-04-22-3f8a`). The user must confirm before the first publish so they can decide whether to proceed with the generated path.

### Safety Scanning

Before creating/updating, content is scanned for potential secret leaks (API keys, tokens). Files matching patterns like `sk-`, `ghp_`, ` Bearer` are skipped with a clear error rather than published.

### Rate Limiting

Telegraph limits API calls. telesync adds a 200ms delay between updates and retries with exponential backoff (1s, 2s, 4s) on failure, up to 3 retries.
