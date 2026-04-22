use std::path::Path;

use serial_test::serial;
use telesync::client::TelegraphClient;
use telesync::config::Config;
use telesync::error::TelesyncError;
use telesync::state::{PageStatus, SyncState};
use telesync::sync::{SyncOptions, run_sync};
use telesync::types::{Node, NodeAttrs, NodeElement};

const TEST_TOKEN: &str = "b3a57cb6d7732ae7bedc0ae33cf60fd303bee3bfdd7b77446e76bbc604b4";

/// Rate limit delay between API calls (ms).
const RATE_LIMIT_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_test_config(dir: &Path, token: &str, publications: &[(&str, &str)]) {
    let mut toml_content = String::new();

    // Account section
    toml_content.push_str("[accounts.test]\n");
    toml_content.push_str("short_name = \"inttest\"\n");
    toml_content.push_str("author_name = \"Integration Test\"\n");
    toml_content.push_str("author_url = \"https://example.com\"\n");
    toml_content.push_str(&format!("access_token = \"{}\"\n\n", token));

    // Publication sections
    for (name, path) in publications {
        toml_content.push_str("[[publications]]\n");
        toml_content.push_str(&format!("name = \"{}\"\n", name));
        toml_content.push_str(&format!("path = \"{}\"\n", path));
        toml_content.push_str("account = \"test\"\n\n");
    }

    std::fs::write(dir.join("telesync.toml"), toml_content).expect("failed to write test config");
}

fn write_md(dir: &Path, rel_path: &str, content: &str) {
    let full_path = dir.join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent dirs");
    }
    std::fs::write(&full_path, content).expect("failed to write markdown file");
}

fn unique_title() -> String {
    format!("Test-{}", uuid::Uuid::new_v4())
}

async fn rate_limit() {
    tokio::time::sleep(std::time::Duration::from_millis(RATE_LIMIT_MS)).await;
}

// ===========================================================================
// 1. Telegraph Client Tests (API)
// ===========================================================================

#[tokio::test]
#[serial]
async fn test_create_account() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let short_name = format!("t{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let account = client
        .create_account(
            &short_name,
            Some("Test Author"),
            Some("https://example.com"),
        )
        .await
        .expect("create_account should succeed");

    assert_eq!(
        account.short_name, short_name,
        "short_name should match what was requested"
    );
    assert!(
        account.access_token.is_some(),
        "new account must return an access_token"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_get_account_info() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());

    let account = client
        .get_account_info(&["short_name", "author_name", "page_count"])
        .await
        .expect("get_account_info should succeed");

    assert!(
        !account.short_name.is_empty(),
        "short_name should be non-empty"
    );
    assert!(
        account.page_count.is_some(),
        "page_count should be present when requested"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_edit_account_info() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let new_name = format!("Author-{}", &uuid::Uuid::new_v4().to_string()[..6]);

    let updated = client
        .edit_account_info(None, Some(&new_name), None)
        .await
        .expect("edit_account_info should succeed");

    assert_eq!(
        updated.author_name, new_name,
        "author_name should be updated"
    );

    // Restore original
    client
        .edit_account_info(None, Some("Integration Test"), None)
        .await
        .expect("restoring author_name should succeed");

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_create_page() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let title = unique_title();

    let content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Hello, Telegraph!".to_string())]),
    })];

    let page = client
        .create_page(&title, Some("Test Author"), None, &content, false)
        .await
        .expect("create_page should succeed");

    assert_eq!(page.title, title, "page title should match");
    assert!(!page.path.is_empty(), "page path should be non-empty");
    assert!(
        page.url.starts_with("https://telegra.ph/"),
        "page URL should be a telegra.ph URL, got: {}",
        page.url
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_create_page_with_rich_content() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let title = unique_title();

    let content = vec![
        // Bold
        Node::Element(NodeElement {
            tag: "p".to_string(),
            attrs: None,
            children: Some(vec![Node::Element(NodeElement {
                tag: "strong".to_string(),
                attrs: None,
                children: Some(vec![Node::Text("Bold text".to_string())]),
            })]),
        }),
        // Italic
        Node::Element(NodeElement {
            tag: "p".to_string(),
            attrs: None,
            children: Some(vec![Node::Element(NodeElement {
                tag: "em".to_string(),
                attrs: None,
                children: Some(vec![Node::Text("Italic text".to_string())]),
            })]),
        }),
        // Link
        Node::Element(NodeElement {
            tag: "p".to_string(),
            attrs: None,
            children: Some(vec![Node::Element(NodeElement {
                tag: "a".to_string(),
                attrs: Some(NodeAttrs {
                    href: Some("https://example.com".to_string()),
                    src: None,
                }),
                children: Some(vec![Node::Text("A link".to_string())]),
            })]),
        }),
        // Unordered list
        Node::Element(NodeElement {
            tag: "ul".to_string(),
            attrs: None,
            children: Some(vec![
                Node::Element(NodeElement {
                    tag: "li".to_string(),
                    attrs: None,
                    children: Some(vec![Node::Text("Item 1".to_string())]),
                }),
                Node::Element(NodeElement {
                    tag: "li".to_string(),
                    attrs: None,
                    children: Some(vec![Node::Text("Item 2".to_string())]),
                }),
            ]),
        }),
        // Blockquote
        Node::Element(NodeElement {
            tag: "blockquote".to_string(),
            attrs: None,
            children: Some(vec![Node::Element(NodeElement {
                tag: "p".to_string(),
                attrs: None,
                children: Some(vec![Node::Text("A quote".to_string())]),
            })]),
        }),
        // Code block
        Node::Element(NodeElement {
            tag: "pre".to_string(),
            attrs: None,
            children: Some(vec![Node::Element(NodeElement {
                tag: "code".to_string(),
                attrs: None,
                children: Some(vec![Node::Text("let x = 42;".to_string())]),
            })]),
        }),
    ];

    let page = client
        .create_page(&title, Some("Test Author"), None, &content, true)
        .await
        .expect("create_page with rich content should succeed");

    assert_eq!(page.title, title);
    assert!(
        page.content.is_some(),
        "content should be returned when return_content=true"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_edit_page() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let title = unique_title();

    let original_content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Original content".to_string())]),
    })];

    let page = client
        .create_page(&title, Some("Test Author"), None, &original_content, false)
        .await
        .expect("create_page should succeed");

    rate_limit().await;

    let new_content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Updated content".to_string())]),
    })];

    let edited = client
        .edit_page(
            &page.path,
            &title,
            Some("Test Author"),
            None,
            &new_content,
            true,
        )
        .await
        .expect("edit_page should succeed");

    assert_eq!(edited.path, page.path, "path should remain the same");
    let content_nodes = edited.content.expect("content should be returned");
    let content_text = extract_text_from_nodes(&content_nodes);
    assert!(
        content_text.contains("Updated content"),
        "edited page should contain updated text, got: {}",
        content_text
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_get_page() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let title = unique_title();

    let content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Get page test".to_string())]),
    })];

    let created = client
        .create_page(&title, Some("Test Author"), None, &content, false)
        .await
        .expect("create_page should succeed");

    rate_limit().await;

    let fetched = client
        .get_page(&created.path, false)
        .await
        .expect("get_page should succeed");

    assert_eq!(fetched.title, title, "fetched title should match");
    assert_eq!(fetched.path, created.path, "fetched path should match");
    assert!(
        fetched.content.is_none(),
        "content should be None when return_content=false"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_get_page_with_content() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let title = unique_title();

    let content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Content for get_page".to_string())]),
    })];

    let created = client
        .create_page(&title, Some("Test Author"), None, &content, false)
        .await
        .expect("create_page should succeed");

    rate_limit().await;

    let fetched = client
        .get_page(&created.path, true)
        .await
        .expect("get_page with content should succeed");

    assert!(
        fetched.content.is_some(),
        "content should be present when return_content=true"
    );
    let nodes = fetched.content.unwrap();
    let text = extract_text_from_nodes(&nodes);
    assert!(
        text.contains("Content for get_page"),
        "fetched content should include original text, got: {}",
        text
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_get_page_list() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());

    // Create 2 pages
    let title1 = unique_title();
    let title2 = unique_title();

    let content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Page list test".to_string())]),
    })];

    client
        .create_page(&title1, Some("Test Author"), None, &content, false)
        .await
        .expect("create first page should succeed");

    rate_limit().await;

    client
        .create_page(&title2, Some("Test Author"), None, &content, false)
        .await
        .expect("create second page should succeed");

    rate_limit().await;

    let page_list = client
        .get_page_list(Some(0), Some(200))
        .await
        .expect("get_page_list should succeed");

    assert!(
        page_list.total_count >= 2,
        "total_count should be at least 2, got: {}",
        page_list.total_count
    );

    let titles: Vec<&str> = page_list.pages.iter().map(|p| p.title.as_str()).collect();
    assert!(
        titles.contains(&title1.as_str()),
        "page list should contain first page title"
    );
    assert!(
        titles.contains(&title2.as_str()),
        "page list should contain second page title"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_get_views() {
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let title = unique_title();

    let content = vec![Node::Element(NodeElement {
        tag: "p".to_string(),
        attrs: None,
        children: Some(vec![Node::Text("Views test".to_string())]),
    })];

    let page = client
        .create_page(&title, Some("Test Author"), None, &content, false)
        .await
        .expect("create_page should succeed");

    rate_limit().await;

    let views = client
        .get_views(&page.path, Some(2026), None, None, None)
        .await
        .expect("get_views should succeed");

    assert!(
        views.views >= 0,
        "views count should be non-negative, got: {}",
        views.views
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_invalid_token() {
    let client = TelegraphClient::new("invalid_token_that_does_not_exist".to_string());

    let result = client.get_account_info(&["short_name"]).await;

    assert!(result.is_err(), "invalid token should produce an error");

    match result.unwrap_err() {
        TelesyncError::Api(msg) => {
            assert!(!msg.is_empty(), "API error message should be non-empty");
        }
        other => panic!("expected TelesyncError::Api, got: {:?}", other),
    }

    rate_limit().await;
}

// ===========================================================================
// 2. Full Sync Lifecycle Tests
// ===========================================================================

#[tokio::test]
#[serial]
async fn test_sync_create() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/hello.md",
        &format!(
            "---\ntitle: {}\n---\nHello world from sync test.\n",
            unique_title()
        ),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("run_sync should succeed");

    assert_eq!(summary.created, 1, "should create 1 page");
    assert_eq!(summary.failed, 0, "should have 0 failures");

    // Verify state was persisted
    let state_after = SyncState::load(&state_path).expect("reload state");
    assert_eq!(
        state_after.pages.len(),
        1,
        "state should contain 1 page entry"
    );

    let (_, page_state) = state_after.pages.iter().next().unwrap();
    assert_eq!(page_state.status, PageStatus::Published);
    assert!(!page_state.telegraph_path.is_empty());

    // Verify page exists on Telegraph
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&page_state.telegraph_path, true)
        .await
        .expect("page should exist on Telegraph");
    assert!(!page.title.is_empty(), "page title should be non-empty");

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_idempotent() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/idem.md",
        &format!("---\ntitle: {}\n---\nIdempotent content.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // First sync
    let summary1 = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("first sync should succeed");
    assert_eq!(summary1.created, 1);

    rate_limit().await;

    // Second sync with same content
    let mut state2 = SyncState::load(&state_path).expect("reload state");
    let summary2 = run_sync(
        &config,
        &mut state2,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("second sync should succeed");

    assert_eq!(
        summary2.up_to_date, 1,
        "second run should report 1 up-to-date"
    );
    assert_eq!(summary2.created, 0, "second run should create 0");
    assert_eq!(summary2.updated, 0, "second run should update 0");

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_update() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/update.md",
        &format!("---\ntitle: {}\n---\nOriginal body.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // First sync
    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("first sync should succeed");

    rate_limit().await;

    // Modify file content
    write_md(
        root,
        "posts/update.md",
        &format!("---\ntitle: {}\n---\nModified body content.\n", title),
    );

    // Second sync
    let mut state2 = SyncState::load(&state_path).expect("reload state");
    let summary2 = run_sync(
        &config,
        &mut state2,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("second sync should succeed");

    assert_eq!(summary2.updated, 1, "should report 1 updated");
    assert_eq!(summary2.created, 0, "should report 0 created");

    // Verify content changed on Telegraph
    let state_after = SyncState::load(&state_path).expect("reload state");
    let (_, ps) = state_after.pages.iter().next().unwrap();
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&ps.telegraph_path, true)
        .await
        .expect("get page");
    let text = extract_text_from_nodes(&page.content.unwrap());
    assert!(
        text.contains("Modified body content"),
        "page should contain updated text, got: {}",
        text
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_delete() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/to-delete.md",
        &format!("---\ntitle: {}\n---\nContent to be removed.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // First sync -- create the page
    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("first sync should succeed");

    rate_limit().await;

    // Remove the file
    std::fs::remove_file(root.join("posts/to-delete.md")).expect("failed to remove markdown file");

    // Second sync -- should delete
    let mut state2 = SyncState::load(&state_path).expect("reload state");
    let summary2 = run_sync(
        &config,
        &mut state2,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("second sync should succeed");

    assert_eq!(summary2.deleted, 1, "should report 1 deleted");

    // Verify state shows deleted
    let state_after = SyncState::load(&state_path).expect("reload state");
    let (_, ps) = state_after.pages.iter().next().unwrap();
    assert_eq!(
        ps.status,
        PageStatus::Deleted,
        "page should be marked deleted in state"
    );

    // Verify Telegraph page shows tombstone
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&ps.telegraph_path, true)
        .await
        .expect("get tombstoned page");
    let text = extract_text_from_nodes(&page.content.unwrap());
    assert!(
        text.contains("Removed on") && text.contains("by telesync"),
        "tombstone page should contain removal notice, got: {}",
        text
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_recreate_after_delete() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/recreate.md",
        &format!("---\ntitle: {}\n---\nFirst version.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // Create
    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("first sync");

    rate_limit().await;

    let state_after_create = SyncState::load(&state_path).expect("reload");
    let original_path = state_after_create
        .pages
        .values()
        .next()
        .unwrap()
        .telegraph_path
        .clone();

    // Delete
    std::fs::remove_file(root.join("posts/recreate.md")).unwrap();
    let mut state2 = SyncState::load(&state_path).expect("reload");
    run_sync(
        &config,
        &mut state2,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("delete sync");

    rate_limit().await;

    // Recreate
    let title2 = unique_title();
    write_md(
        root,
        "posts/recreate.md",
        &format!("---\ntitle: {}\n---\nSecond version.\n", title2),
    );
    let mut state3 = SyncState::load(&state_path).expect("reload");
    let summary3 = run_sync(
        &config,
        &mut state3,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("recreate sync");

    assert_eq!(summary3.created, 1, "recreated file should be a new create");

    // Verify new path is different
    let state_final = SyncState::load(&state_path).expect("reload");
    let ps = state_final
        .pages
        .get("posts/recreate.md")
        .expect("page state should exist");
    assert_eq!(ps.status, PageStatus::Published);
    assert_ne!(
        ps.telegraph_path, original_path,
        "recreated page should have a different Telegraph path"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_multiple_files() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);

    for i in 1..=3 {
        let title = unique_title();
        write_md(
            root,
            &format!("posts/multi-{}.md", i),
            &format!("---\ntitle: {}\n---\nFile number {}.\n", title, i),
        );
    }

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    assert_eq!(summary.created, 3, "should create 3 pages");
    assert_eq!(summary.failed, 0, "should have 0 failures");

    let state_after = SyncState::load(&state_path).expect("reload");
    assert_eq!(state_after.pages.len(), 3, "state should have 3 entries");

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_dry_run() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/dryrun.md",
        &format!("---\ntitle: {}\n---\nDry run content.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: true,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("dry run sync should succeed");

    assert_eq!(summary.created, 1, "dry run should plan 1 create");

    // Verify NO state file was modified (or it's empty)
    let state_after = SyncState::load(&state_path).expect("reload");
    assert!(
        state_after.pages.is_empty(),
        "dry run should not persist any state, found {} entries",
        state_after.pages.len()
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_force_update() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/forced.md",
        &format!("---\ntitle: {}\n---\nForced update content.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // First sync
    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("first sync");

    rate_limit().await;

    // Force update without changing content
    let mut state2 = SyncState::load(&state_path).expect("reload");
    let summary2 = run_sync(
        &config,
        &mut state2,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: Some("posts/forced.md"),
            confirm: true,
        },
    )
    .await
    .expect("force sync");

    assert_eq!(
        summary2.updated, 1,
        "force should trigger update even with same hash"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_first_sync_requires_confirm() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(root, "posts/noconfirm.md", "---\ntitle: Test\n---\nBody.\n");

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let result = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: false,
        },
    )
    .await;

    assert!(result.is_err(), "should fail without confirm");
    let err = match result {
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => e,
    };
    match err {
        TelesyncError::ConfirmationRequired(msg) => {
            assert!(
                msg.contains("first sync"),
                "error should mention first sync, got: {}",
                msg
            );
        }
        other => panic!("expected ConfirmationRequired, got: {:?}", other),
    }

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_content_safety_blocks() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/unsafe.md",
        "---\ntitle: Unsafe\n---\nHere is a key: sk-test123 which should be blocked.\n",
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed but skip unsafe file");

    assert_eq!(summary.skipped, 1, "unsafe file should be skipped");
    assert_eq!(summary.created, 0, "no pages should be created");

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_validation_rejects_table() {
    // Note: pulldown-cmark 0.12 with default options does NOT enable table parsing.
    // Table-like syntax passes through as plain text paragraphs.
    // This test verifies the file is processed without failure (table syntax = text).
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/table.md",
        &format!(
            "---\ntitle: {}\n---\n\n| Col1 | Col2 |\n|------|------|\n| A    | B    |\n",
            title
        ),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    // Table syntax without ENABLE_TABLES is treated as plain text, so it creates successfully
    assert_eq!(
        summary.created, 1,
        "table-like text should be treated as plain content and create a page"
    );
    assert_eq!(summary.failed, 0);

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_validation_rejects_image() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/image.md",
        "---\ntitle: Has Image\n---\n\n![alt text](https://example.com/img.png)\n",
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed but fail the file");

    assert_eq!(summary.failed, 1, "file with image should fail validation");
    assert_eq!(summary.created, 0, "no pages should be created");

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_frontmatter_author_override() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/author.md",
        &format!(
            "---\ntitle: {}\nauthor_name: Custom Author\n---\nContent with custom author.\n",
            title
        ),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    // Verify the page uses custom author
    let state_after = SyncState::load(&state_path).expect("reload");
    let (_, ps) = state_after.pages.iter().next().unwrap();
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&ps.telegraph_path, false)
        .await
        .expect("get page");
    assert_eq!(
        page.author_name.as_deref(),
        Some("Custom Author"),
        "page should use frontmatter author_name, got: {:?}",
        page.author_name
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_title_from_frontmatter() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/fm-title.md",
        &format!("---\ntitle: {}\n---\nBody text here.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    let state_after = SyncState::load(&state_path).expect("reload");
    let (_, ps) = state_after.pages.iter().next().unwrap();
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&ps.telegraph_path, false)
        .await
        .expect("get page");
    assert_eq!(
        page.title, title,
        "page title should match frontmatter title"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_title_from_heading() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let heading = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/heading-title.md",
        &format!("## {}\n\nBody text without frontmatter title.\n", heading),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    let state_after = SyncState::load(&state_path).expect("reload");
    let (_, ps) = state_after.pages.iter().next().unwrap();
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&ps.telegraph_path, false)
        .await
        .expect("get page");
    assert_eq!(
        page.title, heading,
        "page title should come from first heading"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_title_from_filename() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    // No frontmatter title, no heading -- title should derive from filename
    write_md(
        root,
        "posts/2026-04-21-my-article.md",
        "Just body text, no title markers at all.\n",
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    let state_after = SyncState::load(&state_path).expect("reload");
    let (_, ps) = state_after.pages.iter().next().unwrap();
    let client = TelegraphClient::new(TEST_TOKEN.to_string());
    let page = client
        .get_page(&ps.telegraph_path, false)
        .await
        .expect("get page");
    assert_eq!(
        page.title, "My Article",
        "page title should be derived from filename, got: {}",
        page.title
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_content_hash_ignores_frontmatter_whitespace() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();
    let title = unique_title();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts")]);
    write_md(
        root,
        "posts/ws.md",
        &format!("---\ntitle: {}\n---\nBody stays the same.\n", title),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // First sync
    run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("first sync");

    rate_limit().await;

    // Change only frontmatter whitespace (add blank line in frontmatter area)
    write_md(
        root,
        "posts/ws.md",
        &format!("---\ntitle:   {}  \n---\nBody stays the same.\n", title),
    );

    // Second sync -- should be up-to-date since the body, title, and author resolve identically
    let mut state2 = SyncState::load(&state_path).expect("reload");
    let summary2 = run_sync(
        &config,
        &mut state2,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("second sync");

    assert_eq!(
        summary2.up_to_date, 1,
        "whitespace-only frontmatter change should not trigger update"
    );
    assert_eq!(summary2.updated, 0);

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_multiple_publications() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts"), ("notes", "notes")]);

    let title1 = unique_title();
    let title2 = unique_title();
    write_md(
        root,
        "posts/pub1.md",
        &format!("---\ntitle: {}\n---\nBlog post.\n", title1),
    );
    write_md(
        root,
        "notes/pub2.md",
        &format!("---\ntitle: {}\n---\nNote content.\n", title2),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: None,
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("sync should succeed");

    assert_eq!(
        summary.created, 2,
        "should create 2 pages (one per publication)"
    );

    let state_after = SyncState::load(&state_path).expect("reload");
    assert_eq!(state_after.pages.len(), 2);

    // Verify both publications are represented
    let publications: Vec<&str> = state_after
        .pages
        .values()
        .map(|ps| ps.publication.as_str())
        .collect();
    assert!(
        publications.contains(&"blog"),
        "should have blog publication"
    );
    assert!(
        publications.contains(&"notes"),
        "should have notes publication"
    );

    rate_limit().await;
}

#[tokio::test]
#[serial]
async fn test_sync_publication_filter() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let root = tmp.path();

    write_test_config(root, TEST_TOKEN, &[("blog", "posts"), ("notes", "notes")]);

    let title1 = unique_title();
    let title2 = unique_title();
    write_md(
        root,
        "posts/filtered1.md",
        &format!("---\ntitle: {}\n---\nBlog.\n", title1),
    );
    write_md(
        root,
        "notes/filtered2.md",
        &format!("---\ntitle: {}\n---\nNote.\n", title2),
    );

    let config = Config::load(&root.join("telesync.toml")).expect("config load failed");
    let state_path = root.join("state.json");
    let mut state = SyncState::load(&state_path).expect("state load failed");

    // Sync only "blog" publication
    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        root,
        &SyncOptions {
            publication_filter: Some("blog"),
            dry_run: false,
            force_file: None,
            confirm: true,
        },
    )
    .await
    .expect("filtered sync should succeed");

    assert_eq!(summary.created, 1, "should only create 1 page (blog)");

    let state_after = SyncState::load(&state_path).expect("reload");
    assert_eq!(state_after.pages.len(), 1, "state should have 1 entry");
    let ps = state_after.pages.values().next().unwrap();
    assert_eq!(
        ps.publication, "blog",
        "synced page should be from blog publication"
    );

    rate_limit().await;
}

// ===========================================================================
// Utility functions
// ===========================================================================

fn extract_text_from_nodes(nodes: &[Node]) -> String {
    let mut text = String::new();
    for node in nodes {
        extract_text_recursive(node, &mut text);
    }
    text
}

fn extract_text_recursive(node: &Node, output: &mut String) {
    match node {
        Node::Text(t) => output.push_str(t),
        Node::Element(elem) => {
            if let Some(children) = &elem.children {
                for child in children {
                    extract_text_recursive(child, output);
                }
            }
        }
    }
}
