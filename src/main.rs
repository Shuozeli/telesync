use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing::error;
use tracing_subscriber::EnvFilter;

use telesync::client::TelegraphClient;
use telesync::config::Config;
use telesync::error::TelesyncError;
use telesync::state::{Lockfile, SyncState};
use telesync::sync::run_sync;

#[derive(Parser)]
#[command(name = "telesync", about = "Sync markdown files to Telegraph pages")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Sync a specific publication only
    #[arg(long, global = true)]
    publication: Option<String>,

    /// Show planned actions without making changes
    #[arg(long, global = true)]
    dry_run: bool,

    /// Force re-sync a specific file regardless of hash match
    #[arg(long, global = true)]
    force: Option<String>,

    /// Confirm first-time sync (required for initial publish)
    #[arg(long, global = true)]
    confirm: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Reconcile local markdown files with Telegraph pages
    Sync {
        /// Sync a specific publication only
        #[arg(long)]
        publication: Option<String>,

        /// Show planned actions without making changes
        #[arg(long)]
        dry_run: bool,

        /// Force re-sync a specific file regardless of hash match
        #[arg(long)]
        force: Option<String>,

        /// Confirm first-time sync (required for initial publish)
        #[arg(long)]
        confirm: bool,
    },
    /// Account management commands
    Accounts {
        #[command(subcommand)]
        command: AccountCommands,
    },
}

#[derive(Subcommand)]
enum AccountCommands {
    /// Create Telegraph accounts for entries missing access tokens
    Create,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Sync {
            publication,
            dry_run,
            force,
            confirm,
        }) => {
            run_sync_command(
                publication.as_deref(),
                dry_run,
                force.as_deref(),
                confirm,
            )
            .await
        }
        Some(Commands::Accounts { command }) => match command {
            AccountCommands::Create => run_accounts_create().await,
        },
        None => {
            // Default command is sync, using top-level flags
            run_sync_command(
                cli.publication.as_deref(),
                cli.dry_run,
                cli.force.as_deref(),
                cli.confirm,
            )
            .await
        }
    };

    match result {
        Ok(exit_code) => exit_code,
        Err(e) => {
            handle_error(&e);
            ExitCode::FAILURE
        }
    }
}

async fn run_sync_command(
    publication_filter: Option<&str>,
    dry_run: bool,
    force_file: Option<&str>,
    confirm: bool,
) -> Result<ExitCode, TelesyncError> {
    let root_dir = std::env::current_dir().map_err(|e| {
        TelesyncError::Io(e)
    })?;

    let lock_path = root_dir.join(".telesync.lock");
    let config_path = root_dir.join("telesync.toml");
    let state_path = root_dir.join(".telesync-state.json");

    let _lockfile = Lockfile::acquire(&lock_path)?;

    let config = Config::load(&config_path)?;
    let mut state = SyncState::load(&state_path)?;

    let summary = run_sync(
        &config,
        &mut state,
        &state_path,
        &root_dir,
        publication_filter,
        dry_run,
        force_file,
        confirm,
    )
    .await?;

    print_summary(&summary);

    if summary.failed > 0 {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

async fn run_accounts_create() -> Result<ExitCode, TelesyncError> {
    let root_dir = std::env::current_dir().map_err(|e| {
        TelesyncError::Io(e)
    })?;

    let config_path = root_dir.join("telesync.toml");
    let config = Config::load(&config_path)?;

    let mut all_configured = true;

    for (name, account) in &config.accounts {
        let token = account.access_token.trim();
        if token.is_empty() || is_placeholder_token(token) {
            all_configured = false;

            // Create account without needing a token
            let client = TelegraphClient::new(String::new());
            let result = client
                .create_account(
                    &account.short_name,
                    Some(&account.author_name),
                    Some(&account.author_url),
                )
                .await?;

            let new_token = result.access_token.unwrap_or_default();

            println!("Account created: {name}");
            println!("  short_name:   {}", result.short_name);
            println!("  author_name:  {}", result.author_name);
            println!("  author_url:   {}", result.author_url);
            println!("  access_token: {new_token}");
            println!();
            println!("  Update telesync.toml [accounts.{name}] with:");
            println!("    access_token = \"{new_token}\"");
            println!();
        }
    }

    if all_configured {
        println!("All accounts already configured.");
    }

    Ok(ExitCode::SUCCESS)
}

fn is_placeholder_token(token: &str) -> bool {
    let lower = token.to_lowercase();
    lower.contains("placeholder")
        || lower.contains("todo")
        || lower.contains("xxx")
        || lower == "..."
        || lower == "abc123..."
        || lower == "def456..."
        || lower == "ghi789..."
}

fn print_summary(summary: &telesync::sync::SyncSummary) {
    println!("telesync: sync complete");
    println!("  created:    {}", summary.created);
    println!("  updated:    {}", summary.updated);
    println!("  deleted:    {}", summary.deleted);
    println!("  skipped:    {}", summary.skipped);
    println!("  failed:     {}", summary.failed);
    println!("  up-to-date: {}", summary.up_to_date);

    if !summary.errors.is_empty() {
        println!();
        println!("Errors:");
        for err in &summary.errors {
            println!("  - {err}");
        }
    }
}

fn handle_error(err: &TelesyncError) {
    match err {
        TelesyncError::ConfirmationRequired(msg) => {
            error!("{msg}");
            eprintln!("telesync: {msg}");
            eprintln!("telesync: pass --confirm to proceed");
        }
        TelesyncError::Locked { pid } => {
            eprintln!("telesync: lockfile held by PID {pid}");
            eprintln!(
                "telesync: another sync is running, or remove stale .telesync.lock if PID is dead"
            );
        }
        other => {
            error!("{other}");
            eprintln!("telesync: {other}");
        }
    }
}
