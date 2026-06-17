mod bazel;
mod file_churn;
mod files;
mod owner;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::{
    io::{self, Write},
    path::PathBuf,
    time::Instant,
};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Only consider commits after specified date (passed to `git log --since=...`).
    #[arg(short, long)]
    since: Option<String>,

    /// Bazel workspace root (default: auto-detect from current directory).
    #[arg(long)]
    workspace: Option<PathBuf>,

    /// Bazel universe used for dependent analysis.
    #[arg(long, default_value = "//...")]
    universe: String,

    /// Select only top N entries
    #[arg(long)]
    limit: Option<usize>,
}

fn main() -> Result<()> {
    init_logging();

    let Args {
        since,
        workspace,
        universe,
        limit,
    } = Args::parse();

    let workspace = match workspace {
        Some(workspace) => workspace,
        None => bazel::find_workspace_root().context("failed to find Bazel workspace root")?,
    };

    info!(
        workspace = %workspace.display(),
        universe = %universe,
        since = ?since,
        ?limit,
        "starting analysis"
    );

    let started = Instant::now();
    let churn_stats = file_churn::parse_git_log(&workspace, since.as_deref())
        .context("failed to compute git file churn")?;
    info!(
        churned_files = churn_stats.churn.len(),
        malformed_lines = churn_stats.malformed_lines,
        elapsed_ms = started.elapsed().as_millis(),
        "computed git file churn"
    );
    if churn_stats.malformed_lines > 0 {
        warn!(
            malformed_lines = churn_stats.malformed_lines,
            "skipped malformed or unknown git --name-status lines"
        );
    }

    // Filter out non-target files.
    let started = Instant::now();
    let labels_by_path = bazel::resolve_paths_to_labels(&workspace, churn_stats.churn.keys())
        .context("failed to resolve changed paths to Bazel labels")?;
    info!(
        churned_files = churn_stats.churn.len(),
        resolved_labels = labels_by_path.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "resolved changed paths"
    );

    // Count transitive dependents for each resolved label.
    let started = Instant::now();
    let dependent_counts =
        bazel::count_transitive_dependents_by_label(&workspace, &universe, labels_by_path.values())
            .context("failed to count target transitive dependents")?;
    let zero_dependent_targets = dependent_counts
        .values()
        .filter(|&&dependent_count| dependent_count == 0)
        .count();
    info!(
        input_target_labels = labels_by_path.len(),
        zero_dependent_targets,
        elapsed_ms = started.elapsed().as_millis(),
        "counted target transitive dependents"
    );

    let report = report::build_report(
        report::ReportConfig {
            workspace: workspace.display().to_string(),
            universe,
            since,
            limit,
        },
        &churn_stats,
        &labels_by_path,
        &dependent_counts,
    );

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer_pretty(&mut stdout, &report).context("failed to write JSON report")?;
    writeln!(stdout).context("failed to finish writing JSON report")?;

    Ok(())
}

fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(io::stderr)
        .init();
}
