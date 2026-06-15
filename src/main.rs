mod bazel;
mod file_churn;
mod files;
mod report;

use clap::Parser;
use std::{
    error::Error,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Only consider commits after specified date (passed to `git log --since=...`).
    #[arg(short, long)]
    since: Option<String>,

    /// Bazel workspace root (default: auto-detect from current directory).
    #[arg(long)]
    workspace: Option<PathBuf>,

    /// Bazel universe used for reverse dependency analysis.
    #[arg(long, default_value = "//...")]
    universe: String,

    /// Select only top N entries
    #[arg(long)]
    limit: Option<usize>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let Args {
        since,
        workspace,
        universe,
        limit,
    } = Args::parse();

    let workspace = match workspace {
        Some(workspace) => workspace,
        None => bazel::find_workspace_root()?,
    };

    let churn_stats = file_churn::parse_git_log(&workspace, since.as_deref())?;
    if churn_stats.malformed_lines > 0 {
        eprintln!(
            "warning: skipped {} malformed or unknown git --name-status lines",
            churn_stats.malformed_lines
        );
    }

    // Filter out non-target files.
    let path_to_label = bazel::query_paths(&workspace, churn_stats.churn.keys())?;

    // Count transitive dependents for each resolved label.
    let dependents_map = bazel::query_rdeps_counts(&workspace, &universe, path_to_label.values())?;

    let report = report::build_report(
        report::ReportConfig {
            workspace: workspace.display().to_string(),
            universe,
            since,
            limit,
        },
        &churn_stats,
        &path_to_label,
        &dependents_map,
    );

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer_pretty(&mut stdout, &report)?;
    writeln!(stdout)?;

    Ok(())
}
