# telesync Codelabs

## Codelab 1: First Sync

1. Create a `telesync.toml` config with one publication and one account
2. Create a markdown file with frontmatter: `posts/hello.md`
3. Run `cargo run -- sync --dry-run` to preview actions
4. Run `cargo run -- sync --confirm` to publish
5. Verify the page at the Telegraph URL printed in the output

## Codelab 2: Updating and Deleting

1. Edit `posts/hello.md` content or title
2. Run `cargo run -- sync` — telesync detects the hash change and updates
3. Delete `posts/hello.md`
4. Run `cargo run -- sync` — telesync creates a tombstone page
5. Recreate the file with the same or new content and sync again

## Codelab 3: Adding a New Publication

1. Add a second `[[publications]]` entry in `telesync.toml`
2. Create a directory (e.g., `notes/`) and add markdown files
3. Run `cargo run -- sync --publication notes` to sync only that publication
4. Verify with `cargo run -- sync` (syncs all)

## Codelab 4: Debugging Sync Issues

1. Run with `RUST_LOG=debug cargo run -- sync` for verbose output
2. Check `.telesync-state.json` for the current sync state
3. If a page has `status: deleted` but the file exists, run with `--force posts/file.md` to re-publish
4. If Telegraph returns a path collision, manually edit the state file or delete and re-sync
