<!-- agent-updated: 2026-04-21T02:00:00Z -->

# telesync -- Git-to-Telegraph Sync Tool

## Overview

telesync is a Rust CLI tool that syncs markdown files from a git repository to Telegraph (telegra.ph) pages. Git is the source of truth. The tool reconciles local markdown files with published Telegraph pages: creating new pages, updating changed ones, and marking deleted files as removed.

## Core Principles

1. **Git is the source of truth.** No collaboration on Telegraph -- all edits happen in git. The tool never pulls content from Telegraph back to local.
2. **Declarative sync.** The set of markdown files on disk IS the desired state. The tool reconciles Telegraph to match.
3. **Multi-repo, multi-account.** Each repo has its own `.telesync.toml` config. One repo can map multiple directories to different Telegraph accounts.
4. **Secrets in repo.** Tokens stored directly in config (private repos). No external secret management.
5. **Crash-safe state.** State file is flushed after each successful API call, not at the end of sync. Partial failures leave a consistent state.

## Telegraph API Constraints

These constraints are permanent and shape every design decision:

- **No delete API.** Pages are permanent. Can only be edited. Deletion = replace content with tombstone.
- **No visibility control.** All pages are public. Anyone with the URL can read them.
- **No version history.** Edit replaces content entirely. Previous content is gone.
- **64KB content limit** per page.
- **24 allowed HTML tags**, only `href`/`src` attributes. No `<table>`, no `<h1>`/`<h2>`.
- **URL path is deterministic** (`{slugified-title}-{MM}-{DD}`) and never changes after creation.
- **Token rotation breaks editing.** `revokeAccessToken` returns a new token; the old token can no longer edit pages it created. Rotate with caution -- all existing pages become uneditable under the new token.
- **No documented rate limits.** Undocumented limits likely exist. Must be defensive.

## Architecture

### Tool (this repo)

```
telesync/
  src/
    main.rs               # clap CLI entry point
    client.rs             # Telegraph HTTP API client
    types.rs              # API types: Account, Page, Node, NodeElement
    markdown.rs           # pulldown-cmark -> Vec<Node> conversion + validation
    sync.rs               # reconciliation algorithm
    state.rs              # .telesync-state.json read/write (crash-safe)
    config.rs             # .telesync.toml parsing + validation
    error.rs              # TelesyncError enum
  Cargo.toml
  DESIGN.md
```

### User's article repo (any git repo)

```
my-articles/
  .telesync.toml              # config: publications + accounts
  .telesync-state.json        # auto-managed, committed to git
  financial/
    2026-04-21-market-digest.md
    2026-04-20-earnings-preview.md
  legal/
    2026-04-21-legal-brief.md
  entropy/
    2026-04-21-premarket.md
```

## Config File (.telesync.toml)

```toml
# Publications map directories to Telegraph accounts.
# Each publication syncs one directory tree to one account.
# Publication paths MUST NOT overlap (enforced at config load).

[[publications]]
name = "financial"
path = "financial/"
account = "dragb-financial"

[[publications]]
name = "legal"
path = "legal/"
account = "dragb-legal"

[[publications]]
name = "entropy"
path = "entropy/"
account = "dragb-entropy"

# Account definitions.
# Tokens stored directly -- this is a private repo.

[accounts.dragb-financial]
short_name = "dragb-financial"
author_name = "dragb Financial Intelligence"
author_url = "https://uiproxy.yuacx.com"
access_token = "abc123..."

[accounts.dragb-legal]
short_name = "dragb-legal"
author_name = "dragb Legal Brief"
author_url = "https://uiproxy.yuacx.com"
access_token = "def456..."

[accounts.dragb-entropy]
short_name = "dragb-entropy"
author_name = "dragb Entropy Vol"
author_url = "https://uiproxy.yuacx.com"
access_token = "ghi789..."
```

### Config Validation (enforced at load)

1. All publication `account` references must exist in `[accounts.*]`. Fail-fast if missing.
2. Publication paths must not overlap. Two publications claiming the same directory (or a parent/child relationship) is rejected.
3. `access_token` must be non-empty for all accounts.
4. `short_name` must be 1-32 characters per Telegraph API constraint.

## State File (.telesync-state.json)

Auto-managed by the tool. Committed to git as the publication record.

**Critical: state is flushed to disk after each successful API call.** This ensures that if the process crashes mid-sync, the state file accurately reflects what has been published. See "Crash Safety" section below.

```json
{
  "pages": {
    "financial/2026-04-21-market-digest.md": {
      "publication": "financial",
      "telegraph_path": "Daily-Market-Digest-04-21",
      "telegraph_url": "https://telegra.ph/Daily-Market-Digest-04-21",
      "content_hash": "sha256:a1b2c3...",
      "title": "Daily Market Digest",
      "last_synced": "2026-04-21T00:00:00Z",
      "status": "published"
    },
    "financial/2026-04-19-old-article.md": {
      "publication": "financial",
      "telegraph_path": "Old-Article-04-19",
      "telegraph_url": "https://telegra.ph/Old-Article-04-19",
      "content_hash": "sha256:d4e5f6...",
      "title": "Old Article",
      "last_synced": "2026-04-19T00:00:00Z",
      "status": "deleted",
      "deleted_at": "2026-04-21T00:00:00Z"
    }
  }
}
```

State file writes use write-then-rename (`write to .telesync-state.json.tmp`, then `rename` over the original) for atomicity.

## Markdown File Format

```markdown
---
title: "Daily Market Digest"
author_name: "Custom Author Override"   # optional, overrides account default
author_url: "https://custom.url"        # optional, overrides account default
---

## Top Stories

**NVIDIA announces new AI chip** - The company revealed its next-generation
GPU architecture targeting enterprise AI workloads.

> Signal: Geopolitics -> Industrials 5d (MI=0.049)

- Item one
- Item two

### Subsection

1. Ordered item
2. Another item

---

`inline code` and fenced blocks:

\```rust
fn main() {
    println!("Hello");
}
\```
```

### Title Resolution

Priority order:
1. Frontmatter `title` field
2. First `#` or `##` heading in the document
3. Derived from filename: `2026-04-21-market-digest.md` -> `"Market Digest"`

## Markdown -> Telegraph Node Mapping

| Markdown | Telegraph Node | Notes |
|----------|---------------|-------|
| `# H1` | `h3` | Telegraph has no h1/h2; h1 and h2 both map to h3 |
| `## H2` | `h3` | Same as h1 |
| `### H3` | `h4` | |
| `#### H4+` | `h4` with **bold** | Deeper headings use h4 + bold text |
| `**bold**` | `strong` | |
| `*italic*` | `em` | |
| `~~strike~~` | `s` | |
| `[text](url)` | `a` with `href` | |
| `- item` | `ul > li` | |
| `1. item` | `ol > li` | |
| `> quote` | `blockquote` | |
| `` `code` `` | `code` | Inline code |
| Fenced block | `pre > code` | Code blocks |
| `---` | `hr` | |
| Paragraph | `p` | |
| `![alt](url)` | UNSUPPORTED | Rejected by validation |
| Raw HTML | UNSUPPORTED | Rejected by validation |
| Tables | UNSUPPORTED | Rejected by validation (v2: pre block fallback) |
| Footnotes | UNSUPPORTED | Rejected by validation |
| Task lists `- [x]` | UNSUPPORTED | Rejected by validation |

### Unsupported Features (v1)

These markdown features are detected and rejected in v1. Files containing them will not be synced. Future versions may add conversion support.

- **Tables** -- Telegraph has no `<table>` tag. v2 will convert to aligned monospace `pre` blocks.
- **Images** (`![alt](url)`) -- Telegraph requires externally hosted images. Out of scope for v1.
- **Raw HTML** -- Unpredictable and not safely convertible.
- **Footnotes** -- No Telegraph equivalent.
- **Task lists** (`- [x]`, `- [ ]`) -- Would lose checkbox semantics, render as plain list items.

## Markdown Validation

The validation layer scans parsed markdown for unsupported features before conversion. In v1, the only behavior is **reject**: fail sync for that file, log an error with the file path and the unsupported feature found.

```
ERROR: financial/2026-04-21-report.md: unsupported markdown feature: table (line 42)
ERROR: financial/2026-04-21-report.md: unsupported markdown feature: image (line 67)
SKIP:  financial/2026-04-21-report.md (1 error, not synced)
```

Future versions may add `skip` (silently skip the file) and `strip` (remove unsupported elements and proceed) policies, controlled by a `[validation].on_unsupported` config key.

## Content Safety

Telegraph pages are permanently public. The tool scans content before publishing and rejects files matching a built-in denylist of patterns that should never appear on public pages:

- API key patterns (`sk-`, `AIza`, `ghp_`, `GOCSPX-`, common key prefixes)
- Strings matching `password`, `secret`, `access_token`, `private_key` (case-insensitive)
- IPv4 addresses in private ranges (`10.*`, `192.168.*`, `172.16-31.*`)

If any pattern matches, the file is rejected with a clear error:

```
BLOCKED: financial/2026-04-21-report.md: content safety: possible API key detected (line 15)
```

This is a safety net, not a substitute for good judgment about what to publish.

## Sync Algorithm

```
telesync sync [--publication NAME] [--dry-run] [--force FILE] [--confirm]

1. Acquire lockfile (.telesync.lock). Fail if already held.
2. Load .telesync.toml (fail if missing). Validate config.
3. Load .telesync-state.json (create empty if missing).
4. If this is the first sync for any publication (no state entries),
   require --confirm flag. Prevents accidental mass-publish.
5. For each publication (or filtered by --publication):

   a. Resolve account from config.
   b. Scan publication.path/ recursively for *.md files.
   c. Build the plan (no API calls yet):
      For each .md file:
      - Parse frontmatter + body.
      - Run validation. If rejected, add to skip list.
      - Compute content_hash = sha256(rendered nodes + title + author).
      - Classify:
        * NOT in state                    -> plan: CREATE
        * In state, hash differs          -> plan: UPDATE
        * In state, hash same             -> plan: UP-TO-DATE
        * In state, status=deleted, file reappeared -> plan: CREATE (new URL)
        * --force FILE matches            -> plan: UPDATE (regardless of hash)
      For each state entry where status=published and file missing on disk:
        -> plan: DELETE

   d. If --dry-run, print the plan and exit.

   e. Execute the plan (with API calls):
      For each planned operation:
      - CREATE: validate size (<64KB), createPage, add to state, flush state.
      - UPDATE: validate size (<64KB), editPage, update state, flush state.
      - DELETE: editPage with tombstone, set status=deleted, flush state.
      - Between each API call: wait 200ms (rate limit defense).
      - On API error: retry up to 3 times with exponential backoff (1s, 2s, 4s).
      - On createPage timeout: query getPageList to check if page was created
        before retrying. Prevents duplicate page creation.
      - On persistent failure: log error, skip file, continue with next.

6. Release lockfile.
7. Print summary: created N, updated N, deleted N, skipped N, failed N, up-to-date N.
```

### Content Hash

The content hash is computed over the **rendered** Telegraph node tree (serialized JSON) plus the resolved title and author name. This means:

- Frontmatter whitespace changes (key reordering, trailing spaces) do NOT trigger updates.
- Line ending differences (CRLF vs LF) do NOT trigger updates.
- Only changes to the actual published content trigger updates.

```
content_hash = sha256(
  canonical_json(rendered_nodes) + "\n" + title + "\n" + author_name
)
```

### Crash Safety

State is flushed to disk after every successful API call using write-then-rename:

1. Serialize state to JSON.
2. Write to `.telesync-state.json.tmp`.
3. `rename` over `.telesync-state.json`.

If the process crashes at any point:
- Before step 3: old state file is intact. The page may have been created/updated on Telegraph but is not in the state. Next sync will see it as "NOT in state" and attempt to create it. For creates, this risks a duplicate (see timeout handling above). For updates, editPage is idempotent.
- After step 3: state is consistent.

### Duplicate Detection on Create Timeout

If `createPage` returns a timeout or ambiguous error, before retrying:

1. Call `getPageList` for the account.
2. Search for a page with a matching title.
3. If found, record it in state as published (no duplicate created).
4. If not found, retry the create.

### Lockfile

`.telesync.lock` prevents concurrent sync runs. The lock contains the PID and start timestamp. On startup, if the lock exists:
- Check if the PID is still running. If not, the lock is stale -- remove and proceed.
- If the PID is running, fail with an error.

### Rate Limiting

Between each API call, wait 200ms. On HTTP 429 or 5xx responses, use exponential backoff:
- Attempt 1: wait 1s
- Attempt 2: wait 2s
- Attempt 3: wait 4s
- After 3 failures: skip the file, log error, continue.

### 64KB Limit Enforcement

Before each `createPage` or `editPage` call, check the serialized content size. If it exceeds 60KB (leaving margin for title/author overhead):

```
ERROR: financial/2026-04-21-huge-report.md: content too large (72KB, limit 64KB)
SKIP:  financial/2026-04-21-huge-report.md (not synced)
```

The file is not synced. Future versions may support splitting into linked pages.

## Deleted Page Content

When a file is removed from git, the corresponding Telegraph page is edited to contain:

```json
[
  {"tag": "p", "children": [
    {"tag": "em", "children": ["Removed on 2026-04-21 by telesync."]}
  ]}
]
```

The page URL continues to resolve but shows only the removal notice.

## CLI Commands (v1)

```bash
# Core workflow
telesync sync                              # reconcile all publications
telesync sync --publication financial      # sync one publication only
telesync sync --dry-run                    # show plan, no changes
telesync sync --confirm                    # required for first sync (safety)
telesync sync --force path/to/file.md      # force re-sync regardless of hash

# Account management
telesync accounts create                   # create Telegraph accounts, print tokens
```

### Deferred to v2

```bash
telesync status                            # divergence summary
telesync pages                             # list published pages with URLs
telesync pages --views                     # include view counts
telesync accounts                          # list accounts + page counts
```

## Dependencies

```toml
[dependencies]
reqwest = { version = "0.12", features = ["rustls-tls", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
pulldown-cmark = "0.12"
sha2 = "0.10"
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

## Build Order

1. **types.rs + client.rs** -- Telegraph API client, tested against live API
2. **markdown.rs** -- markdown parsing + node conversion + validation
3. **config.rs + state.rs** -- TOML config + JSON state (crash-safe writes)
4. **sync.rs** -- reconciliation logic with retry, rate limiting, lockfile
5. **main.rs** -- clap CLI
6. **Integration test** -- end-to-end sync with test markdown files

## Known Limitations

- **Token rotation breaks editing.** If you rotate a token via `revokeAccessToken`, all pages created with the old token become uneditable. The new token cannot edit old pages. Do not rotate tokens unless you accept that existing pages are frozen.
- **No move detection.** Renaming or moving a markdown file looks like a delete + create. The old page gets a tombstone; a new page is created at a new URL. This is silent link rot. Content-hash-based move detection is a v2 feature.
- **No table support.** Markdown tables are rejected in v1. v2 will convert them to aligned monospace `pre` blocks.
- **Pages are permanently public.** There is no way to make a Telegraph page private or restrict access. Do not publish sensitive content.
- **No concurrent sync.** The lockfile prevents parallel runs. This is intentional -- concurrent syncs risk state corruption and duplicate pages.

## Future Possibilities (not in v1)

- `telesync watch` -- watch filesystem for changes, auto-sync on save
- CI/CD integration -- GitHub Action / git hook that runs `telesync sync` on push
- View analytics -- periodic `getViews` polling, aggregate stats per publication
- Table rendering -- markdown tables to aligned monospace `pre` blocks
- Content-hash move detection -- reuse Telegraph page when file is renamed
- Telegram notification after sync -- post URLs to chat via notification-service
- `skip` and `strip` validation policies for unsupported markdown features

## Design Review Notes (2026-04-21)

This design was reviewed by four specialized agents (domain expert, systems engineer, pragmatist, security reviewer). Key changes incorporated from the review:

1. **Crash-safe state writes** -- flush after each API call, not at end of sync (P0, domain + systems)
2. **Content hash on rendered output** -- hash post-parse nodes, not raw file bytes (P1, systems)
3. **Retry with exponential backoff** -- defensive rate limiting + retry on failures (P0, systems)
4. **Duplicate detection on timeout** -- query getPageList before retrying createPage (P1, systems)
5. **Config validation** -- overlapping paths rejected, missing account references fail-fast (P1, domain)
6. **64KB limit enforcement** -- validate before API call (P1, domain)
7. **Content safety denylist** -- scan for credential patterns before publishing (P1, security)
8. **First-sync confirmation** -- `--confirm` flag required for initial publish (P1, security)
9. **Lockfile for concurrency** -- PID-based lock prevents parallel runs (P2, systems)
10. **v1 scope cut** -- reject-only validation, no tables, no status/pages commands (pragmatist)
11. **Token rotation warning** -- documented as known limitation (P1, security + domain)
12. **`--force` flag** -- re-sync specific file regardless of hash match (P1, domain)
