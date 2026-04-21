use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use tracing::{error, info, warn};

use crate::client::TelegraphClient;
use crate::config::{AccountConfig, Config, Publication};
use crate::error::TelesyncError;
use crate::markdown::{
    check_content_safety, content_hash, extract_frontmatter, markdown_to_nodes, resolve_title,
};
use crate::state::{PageState, PageStatus, SyncState};
use crate::types::{Node, NodeElement};

// --- Plan types ---

pub enum PlannedAction {
    Create {
        file: String,
        title: String,
        nodes: Vec<Node>,
        author_name: String,
        author_url: String,
    },
    Update {
        file: String,
        path: String,
        title: String,
        nodes: Vec<Node>,
        author_name: String,
        author_url: String,
        new_hash: String,
    },
    Delete {
        file: String,
        path: String,
    },
    UpToDate {
        file: String,
    },
    Skipped {
        file: String,
        reason: String,
    },
    Failed {
        file: String,
        reason: String,
    },
}

pub struct SyncPlan {
    pub publication: String,
    pub actions: Vec<PlannedAction>,
}

pub struct SyncSummary {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub up_to_date: usize,
    pub errors: Vec<String>,
}

impl SyncSummary {
    fn empty() -> Self {
        Self {
            created: 0,
            updated: 0,
            deleted: 0,
            skipped: 0,
            failed: 0,
            up_to_date: 0,
            errors: Vec::new(),
        }
    }

    fn merge(&mut self, other: SyncSummary) {
        self.created += other.created;
        self.updated += other.updated;
        self.deleted += other.deleted;
        self.skipped += other.skipped;
        self.failed += other.failed;
        self.up_to_date += other.up_to_date;
        self.errors.extend(other.errors);
    }
}

// --- File discovery ---

/// Recursively find all .md files under `dir`, returning paths relative to `root_dir`.
fn find_markdown_files(dir: &Path, root_dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    collect_markdown_files_recursive(dir, root_dir, &mut results);
    results.sort();
    results
}

fn collect_markdown_files_recursive(dir: &Path, root_dir: &Path, results: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(dir = %dir.display(), error = %e, "failed to read directory");
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "failed to read directory entry");
                continue;
            }
        };

        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to get file type");
                continue;
            }
        };

        if file_type.is_dir() {
            collect_markdown_files_recursive(&path, root_dir, results);
        } else if file_type.is_file()
            && let Some(ext) = path.extension()
            && ext == "md"
            && let Ok(relative) = path.strip_prefix(root_dir)
        {
            results.push(relative.to_path_buf());
        }
    }
}

// --- Planning ---

pub fn plan_sync(
    publication: &Publication,
    account: &AccountConfig,
    state: &SyncState,
    root_dir: &Path,
    force_file: Option<&str>,
) -> SyncPlan {
    let pub_dir = root_dir.join(&publication.path);
    let md_files = find_markdown_files(&pub_dir, root_dir);

    let mut actions: Vec<PlannedAction> = Vec::new();
    let mut seen_files: HashSet<String> = HashSet::new();

    for relative_path in &md_files {
        let file_key = relative_path.to_string_lossy().to_string();
        // Normalize path separators to forward slash for state keys
        let file_key = file_key.replace('\\', "/");
        seen_files.insert(file_key.clone());

        let abs_path = root_dir.join(relative_path);
        let filename = relative_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        // Read file content
        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(e) => {
                actions.push(PlannedAction::Failed {
                    file: file_key,
                    reason: format!("failed to read file: {}", e),
                });
                continue;
            }
        };

        // Content safety check
        if let Err(e) = check_content_safety(&content, &file_key) {
            actions.push(PlannedAction::Skipped {
                file: file_key,
                reason: format!("content safety: {}", e),
            });
            continue;
        }

        // Parse frontmatter
        let (frontmatter, body) = extract_frontmatter(&content);

        // Resolve title
        let title = resolve_title(&frontmatter, body, &filename);

        // Resolve author (frontmatter overrides account defaults)
        let author_name = frontmatter
            .author_name
            .unwrap_or_else(|| account.author_name.clone());
        let author_url = frontmatter
            .author_url
            .unwrap_or_else(|| account.author_url.clone());

        // Convert markdown to nodes
        let nodes = match markdown_to_nodes(body) {
            Ok(nodes) => nodes,
            Err(issues) => {
                let reasons: Vec<String> = issues
                    .iter()
                    .map(|i| format!("{} (line {})", i.feature, i.line))
                    .collect();
                actions.push(PlannedAction::Failed {
                    file: file_key,
                    reason: format!("unsupported markdown: {}", reasons.join(", ")),
                });
                continue;
            }
        };

        // Compute hash
        let hash = content_hash(&nodes, &title, &author_name);

        // Determine action based on state
        let is_forced = force_file.map(|f| f == file_key).unwrap_or(false);

        match state.pages.get(&file_key) {
            None => {
                // New file, not in state -> Create
                actions.push(PlannedAction::Create {
                    file: file_key,
                    title,
                    nodes,
                    author_name,
                    author_url,
                });
            }
            Some(page_state) => match page_state.status {
                PageStatus::Published => {
                    if is_forced || hash != page_state.content_hash {
                        actions.push(PlannedAction::Update {
                            file: file_key,
                            path: page_state.telegraph_path.clone(),
                            title,
                            nodes,
                            author_name,
                            author_url,
                            new_hash: hash,
                        });
                    } else {
                        actions.push(PlannedAction::UpToDate { file: file_key });
                    }
                }
                PageStatus::Deleted => {
                    // File reappeared after deletion -> Create (new page, new URL)
                    actions.push(PlannedAction::Create {
                        file: file_key,
                        title,
                        nodes,
                        author_name,
                        author_url,
                    });
                }
            },
        }
    }

    // Check for files in state (published) but missing from disk -> Delete
    for (file_key, page_state) in &state.pages {
        if page_state.publication != publication.name {
            continue;
        }
        if page_state.status != PageStatus::Published {
            continue;
        }
        if seen_files.contains(file_key) {
            continue;
        }
        actions.push(PlannedAction::Delete {
            file: file_key.clone(),
            path: page_state.telegraph_path.clone(),
        });
    }

    SyncPlan {
        publication: publication.name.clone(),
        actions,
    }
}

// --- Execution ---

const MAX_CONTENT_SIZE_BYTES: usize = 60 * 1024; // 60KB safety margin under 64KB limit
const RATE_LIMIT_DELAY_MS: u64 = 200;
const MAX_RETRIES: usize = 3;

pub async fn execute_plan(
    plan: &SyncPlan,
    client: &TelegraphClient,
    account: &AccountConfig,
    state: &mut SyncState,
    state_path: &Path,
) -> SyncSummary {
    let _ = account; // account info already embedded in planned actions
    let mut summary = SyncSummary::empty();

    for action in &plan.actions {
        match action {
            PlannedAction::Create {
                file,
                title,
                nodes,
                author_name,
                author_url,
            } => {
                let result = execute_create(
                    file,
                    title,
                    nodes,
                    author_name,
                    author_url,
                    &plan.publication,
                    client,
                    state,
                    state_path,
                )
                .await;

                match result {
                    Ok(()) => {
                        summary.created += 1;
                        info!(file = %file, "created");
                    }
                    Err(e) => {
                        let msg = format!("{}: {}", file, e);
                        error!(file = %file, error = %e, "create failed");
                        summary.failed += 1;
                        summary.errors.push(msg);
                    }
                }

                tokio::time::sleep(Duration::from_millis(RATE_LIMIT_DELAY_MS)).await;
            }

            PlannedAction::Update {
                file,
                path,
                title,
                nodes,
                author_name,
                author_url,
                new_hash,
            } => {
                let result = execute_update(
                    file,
                    path,
                    title,
                    nodes,
                    author_name,
                    author_url,
                    new_hash,
                    client,
                    state,
                    state_path,
                )
                .await;

                match result {
                    Ok(()) => {
                        summary.updated += 1;
                        info!(file = %file, "updated");
                    }
                    Err(e) => {
                        let msg = format!("{}: {}", file, e);
                        error!(file = %file, error = %e, "update failed");
                        summary.failed += 1;
                        summary.errors.push(msg);
                    }
                }

                tokio::time::sleep(Duration::from_millis(RATE_LIMIT_DELAY_MS)).await;
            }

            PlannedAction::Delete { file, path } => {
                let result =
                    execute_delete(file, path, &plan.publication, client, state, state_path).await;

                match result {
                    Ok(()) => {
                        summary.deleted += 1;
                        info!(file = %file, "deleted (tombstoned)");
                    }
                    Err(e) => {
                        let msg = format!("{}: {}", file, e);
                        error!(file = %file, error = %e, "delete failed");
                        summary.failed += 1;
                        summary.errors.push(msg);
                    }
                }

                tokio::time::sleep(Duration::from_millis(RATE_LIMIT_DELAY_MS)).await;
            }

            PlannedAction::UpToDate { file } => {
                summary.up_to_date += 1;
                info!(file = %file, "up-to-date");
            }

            PlannedAction::Skipped { file, reason } => {
                summary.skipped += 1;
                warn!(file = %file, reason = %reason, "skipped");
            }

            PlannedAction::Failed { file, reason } => {
                summary.failed += 1;
                let msg = format!("{}: {}", file, reason);
                error!(file = %file, reason = %reason, "failed during planning");
                summary.errors.push(msg);
            }
        }
    }

    summary
}

#[allow(clippy::too_many_arguments)]
async fn execute_create(
    file: &str,
    title: &str,
    nodes: &[Node],
    author_name: &str,
    author_url: &str,
    publication: &str,
    client: &TelegraphClient,
    state: &mut SyncState,
    state_path: &Path,
) -> Result<(), TelesyncError> {
    validate_content_size(file, nodes)?;

    let mut last_error: Option<TelesyncError> = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff = Duration::from_secs(1 << (attempt - 1));
            warn!(
                file = %file,
                attempt = attempt,
                backoff_secs = backoff.as_secs(),
                "retrying create after backoff"
            );
            tokio::time::sleep(backoff).await;

            // Duplicate detection: check if page was already created
            if let Some(page) = detect_duplicate_page(client, title).await {
                let hash = content_hash(nodes, title, author_name);
                let now = Utc::now().to_rfc3339();
                state.pages.insert(
                    file.to_string(),
                    PageState {
                        publication: publication.to_string(),
                        telegraph_path: page.path.clone(),
                        telegraph_url: page.url.clone(),
                        content_hash: hash,
                        title: title.to_string(),
                        last_synced: now,
                        status: PageStatus::Published,
                        deleted_at: None,
                    },
                );
                state.save(state_path)?;
                info!(file = %file, path = %page.path, "duplicate detected, recorded existing page");
                return Ok(());
            }
        }

        match client
            .create_page(title, Some(author_name), Some(author_url), nodes, false)
            .await
        {
            Ok(page) => {
                let hash = content_hash(nodes, title, author_name);
                let now = Utc::now().to_rfc3339();
                state.pages.insert(
                    file.to_string(),
                    PageState {
                        publication: publication.to_string(),
                        telegraph_path: page.path.clone(),
                        telegraph_url: page.url.clone(),
                        content_hash: hash,
                        title: title.to_string(),
                        last_synced: now,
                        status: PageStatus::Published,
                        deleted_at: None,
                    },
                );
                state.save(state_path)?;
                return Ok(());
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| TelesyncError::Api("create failed with no error".to_string())))
}

#[allow(clippy::too_many_arguments)]
async fn execute_update(
    file: &str,
    path: &str,
    title: &str,
    nodes: &[Node],
    author_name: &str,
    author_url: &str,
    new_hash: &str,
    client: &TelegraphClient,
    state: &mut SyncState,
    state_path: &Path,
) -> Result<(), TelesyncError> {
    validate_content_size(file, nodes)?;

    let mut last_error: Option<TelesyncError> = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff = Duration::from_secs(1 << (attempt - 1));
            warn!(
                file = %file,
                attempt = attempt,
                backoff_secs = backoff.as_secs(),
                "retrying update after backoff"
            );
            tokio::time::sleep(backoff).await;
        }

        match client
            .edit_page(
                path,
                title,
                Some(author_name),
                Some(author_url),
                nodes,
                false,
            )
            .await
        {
            Ok(page) => {
                let now = Utc::now().to_rfc3339();
                if let Some(page_state) = state.pages.get_mut(file) {
                    page_state.content_hash = new_hash.to_string();
                    page_state.title = title.to_string();
                    page_state.last_synced = now;
                    page_state.telegraph_url = page.url;
                } else {
                    // State entry was missing; re-create it
                    state.pages.insert(
                        file.to_string(),
                        PageState {
                            publication: String::new(),
                            telegraph_path: path.to_string(),
                            telegraph_url: page.url,
                            content_hash: new_hash.to_string(),
                            title: title.to_string(),
                            last_synced: now,
                            status: PageStatus::Published,
                            deleted_at: None,
                        },
                    );
                }
                state.save(state_path)?;
                return Ok(());
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| TelesyncError::Api("update failed with no error".to_string())))
}

async fn execute_delete(
    file: &str,
    path: &str,
    publication: &str,
    client: &TelegraphClient,
    state: &mut SyncState,
    state_path: &Path,
) -> Result<(), TelesyncError> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let tombstone_content = build_tombstone_nodes(&today);

    let mut last_error: Option<TelesyncError> = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff = Duration::from_secs(1 << (attempt - 1));
            warn!(
                file = %file,
                attempt = attempt,
                backoff_secs = backoff.as_secs(),
                "retrying delete after backoff"
            );
            tokio::time::sleep(backoff).await;
        }

        // Retrieve the current title from state for the edit call
        let title = state
            .pages
            .get(file)
            .map(|ps| ps.title.clone())
            .unwrap_or_else(|| "Removed".to_string());

        match client
            .edit_page(path, &title, None, None, &tombstone_content, false)
            .await
        {
            Ok(_) => {
                let now = Utc::now().to_rfc3339();
                if let Some(page_state) = state.pages.get_mut(file) {
                    page_state.status = PageStatus::Deleted;
                    page_state.deleted_at = Some(now.clone());
                    page_state.last_synced = now;
                } else {
                    state.pages.insert(
                        file.to_string(),
                        PageState {
                            publication: publication.to_string(),
                            telegraph_path: path.to_string(),
                            telegraph_url: format!("https://telegra.ph/{}", path),
                            content_hash: String::new(),
                            title,
                            last_synced: now.clone(),
                            status: PageStatus::Deleted,
                            deleted_at: Some(now),
                        },
                    );
                }
                state.save(state_path)?;
                return Ok(());
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| TelesyncError::Api("delete failed with no error".to_string())))
}

fn build_tombstone_nodes(date: &str) -> Vec<Node> {
    vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Element(NodeElement {
            tag: "em".to_string(),
            attrs: None,
            children: Some(vec![Node::Text(format!(
                "Removed on {} by telesync.",
                date
            ))]),
        })]),
    })]
}

fn validate_content_size(file: &str, nodes: &[Node]) -> Result<(), TelesyncError> {
    let serialized = serde_json::to_string(nodes).unwrap_or_default();
    let size = serialized.len();
    if size > MAX_CONTENT_SIZE_BYTES {
        return Err(TelesyncError::ContentTooLarge {
            file: file.to_string(),
            size_kb: size / 1024,
        });
    }
    Ok(())
}

/// Check if a page with the given title already exists in the account's page list.
/// Used for duplicate detection after a create timeout.
async fn detect_duplicate_page(
    client: &TelegraphClient,
    title: &str,
) -> Option<crate::types::Page> {
    match client.get_page_list(Some(0), Some(200)).await {
        Ok(page_list) => page_list.pages.into_iter().find(|page| page.title == title),
        Err(e) => {
            warn!(error = %e, "failed to fetch page list for duplicate detection");
            None
        }
    }
}

// --- Top-level orchestration ---

#[allow(clippy::too_many_arguments)]
pub async fn run_sync(
    config: &Config,
    state: &mut SyncState,
    state_path: &Path,
    root_dir: &Path,
    publication_filter: Option<&str>,
    dry_run: bool,
    force_file: Option<&str>,
    confirm: bool,
) -> Result<SyncSummary, TelesyncError> {
    let publications: Vec<&Publication> = match publication_filter {
        Some(name) => {
            let matched: Vec<&Publication> = config
                .publications
                .iter()
                .filter(|p| p.name == name)
                .collect();
            if matched.is_empty() {
                return Err(TelesyncError::Config(format!(
                    "no publication named '{}'",
                    name
                )));
            }
            matched
        }
        None => config.publications.iter().collect(),
    };

    let mut aggregate = SyncSummary::empty();

    for publication in publications {
        let account = config.accounts.get(&publication.account).ok_or_else(|| {
            TelesyncError::Config(format!(
                "publication '{}' references unknown account '{}'",
                publication.name, publication.account
            ))
        })?;

        // Check if this is a first sync (no existing state entries for this publication)
        let has_existing_state = state
            .pages
            .values()
            .any(|ps| ps.publication == publication.name);

        if !has_existing_state && !confirm {
            return Err(TelesyncError::ConfirmationRequired(format!(
                "first sync for publication '{}' -- pass --confirm to proceed",
                publication.name
            )));
        }

        let plan = plan_sync(publication, account, state, root_dir, force_file);

        if dry_run {
            print_plan(&plan);
            let plan_summary = summarize_plan(&plan);
            aggregate.merge(plan_summary);
            continue;
        }

        let client = TelegraphClient::new(account.access_token.clone());
        let summary = execute_plan(&plan, &client, account, state, state_path).await;
        aggregate.merge(summary);
    }

    Ok(aggregate)
}

fn print_plan(plan: &SyncPlan) {
    info!(publication = %plan.publication, "--- Sync Plan ---");
    for action in &plan.actions {
        match action {
            PlannedAction::Create { file, title, .. } => {
                info!(file = %file, title = %title, "CREATE");
            }
            PlannedAction::Update { file, title, .. } => {
                info!(file = %file, title = %title, "UPDATE");
            }
            PlannedAction::Delete { file, path } => {
                info!(file = %file, path = %path, "DELETE");
            }
            PlannedAction::UpToDate { file } => {
                info!(file = %file, "UP-TO-DATE");
            }
            PlannedAction::Skipped { file, reason } => {
                warn!(file = %file, reason = %reason, "SKIP");
            }
            PlannedAction::Failed { file, reason } => {
                error!(file = %file, reason = %reason, "FAIL");
            }
        }
    }
}

fn summarize_plan(plan: &SyncPlan) -> SyncSummary {
    let mut summary = SyncSummary::empty();
    for action in &plan.actions {
        match action {
            PlannedAction::Create { .. } => summary.created += 1,
            PlannedAction::Update { .. } => summary.updated += 1,
            PlannedAction::Delete { .. } => summary.deleted += 1,
            PlannedAction::UpToDate { .. } => summary.up_to_date += 1,
            PlannedAction::Skipped { .. } => summary.skipped += 1,
            PlannedAction::Failed { file, reason } => {
                summary.failed += 1;
                summary.errors.push(format!("{}: {}", file, reason));
            }
        }
    }
    summary
}
