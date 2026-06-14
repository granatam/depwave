mod bazel;
mod file_churn;
mod files;
mod report;

use clap::Parser;
use std::{error::Error, path::PathBuf};

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
    let args = Args::parse();

    let workspace = match args.workspace {
        Some(w) => w,
        None => bazel::find_workspace_root()?,
    };

    let file_churn = file_churn::parse_git_log(&workspace, args.since.as_deref())?;
    if file_churn.malformed_lines > 0 {
        eprintln!(
            "warning: skipped {} malformed or unknown git --name-status lines",
            file_churn.malformed_lines
        );
    }

    // Filter out non-target files.
    let path_to_label = bazel::query_paths(&workspace, file_churn.churn.keys())?;

    // Count transitive dependents for each resolved label.
    let dependents_map =
        bazel::query_rdeps_counts(&workspace, &args.universe, path_to_label.values())?;

    let report = report::build_report(
        report::ReportConfig {
            workspace: workspace.display().to_string(),
            universe: args.universe,
            since: args.since,
            limit: args.limit,
        },
        &file_churn,
        &path_to_label,
        &dependents_map,
    );

    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}
