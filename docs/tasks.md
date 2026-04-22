# telesync Tasks

## Completed

- [x] Core sync engine (plan + execute)
- [x] Markdown parsing (frontmatter, body, title extraction)
- [x] Telegraph client (createAccount, createPage, editPage, getPage, getPageList)
- [x] Content hashing (SHA256-based, whitespace-normalized)
- [x] State persistence (`.telesync-state.json`)
- [x] Lockfile (PID-based, stale lock detection)
- [x] Delete with tombstone
- [x] Safety scanning (secret detection)
- [x] CLI (`sync`, `accounts create`)
- [x] Clippy cleanup (struct bundling for too_many_arguments)
- [x] CI (GitHub Actions: fmt, clippy, tests)
- [x] Pre-commit hooks (rustfmt, clippy)

## Pending

- [ ] Support frontmatter `published_date` field
- [ ] HTML entity decoding in page titles (Telegraph returns encoded titles)
- [ ] Handle Telegraph path collisions gracefully
- [ ] `telesync delete --force` to permanently delete a page (if Telegraph ever supports it)
- [ ] `telesync diff` to show pending changes before syncing
- [ ] Watch mode (`telesync sync --watch`)
- [ ] Multiple account support with round-robin or per-publication assignment
