mod bazel;
mod file_churn;
mod files;
mod owner;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::{
    collections::HashMap,
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

    let source_churn: HashMap<String, u64> = churn_stats
        .churn
        .iter()
        .filter(|(path, _)| files::classify_file(path) == files::FileKind::Source)
        .map(|(path, churn)| (path.clone(), *churn))
        .collect();

    let started = Instant::now();
    let labels_by_path = bazel::resolve_paths_to_labels(&workspace, source_churn.keys())
        .context("failed to resolve changed paths to Bazel labels")?;
    info!(
        source_files = source_churn.len(),
        resolved_labels = labels_by_path.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "resolved changed source paths"
    );

    let started = Instant::now();
    let graph = bazel::query_rdeps_graph(&workspace, &universe, labels_by_path.values())
        .context("failed to query Bazel reverse dependency graph")?;
    let (owner_churn, unresolved_source_files, no_owner_source_files) =
        owner::aggregate_owner_churn(&source_churn, &labels_by_path, &graph);
    let owner_impacts = owner::build_owner_impacts(owner_churn, &graph);
    let zero_dependent_targets = owner_impacts
        .iter()
        .filter(|impact| impact.transitive_dependents == 0)
        .count();
    info!(
        input_file_labels = labels_by_path.len(),
        owner_targets = owner_impacts.len(),
        zero_dependent_targets,
        unresolved_source_files = unresolved_source_files.len(),
        no_owner_source_files = no_owner_source_files.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "computed owner target impact"
    );

    let report = report::build_report(
        report::ReportConfig {
            workspace: workspace.display().to_string(),
            universe,
            since,
            limit,
        },
        &churn_stats,
        owner_impacts,
        unresolved_source_files,
        no_owner_source_files,
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
