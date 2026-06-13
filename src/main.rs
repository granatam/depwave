mod bazel;
mod file_churn;
mod report;

use clap::Parser;
use std::{error::Error, path::PathBuf};

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Only consider commits after this (passed to `git log --since=...`).
    #[arg(short, long)]
    since: Option<String>,

    /// Bazel workspace root (default: auto-detect from current directory).
    #[arg(long)]
    workspace: Option<PathBuf>,

    /// Bazel universe used for reverse dependency analysis.
    #[arg(long, default_value = "//...")]
    universe: String,
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

    // Count transitive reverse dependencies for each resolved label.
    let dependents_map =
        bazel::query_rdeps_counts(&workspace, &args.universe, path_to_label.values())?;

    let mut entries: Vec<_> = path_to_label
        .iter()
        .map(|(path, label)| {
            let dependents = dependents_map.get(label.as_str()).copied().unwrap_or(0);
            let churn = file_churn.churn.get(path).copied().unwrap_or(0);
            report::TargetImpact {
                source_path: path.clone(),
                target_label: label.clone(),
                churn,
                dependents,
                impact_score: churn * dependents,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.impact_score
            .cmp(&a.impact_score)
            .then_with(|| b.dependents.cmp(&a.dependents))
            .then_with(|| b.churn.cmp(&a.churn))
            .then_with(|| a.source_path.cmp(&b.source_path))
            .then_with(|| a.target_label.cmp(&b.target_label))
    });

    let report = report::Report {
        workspace: workspace.display().to_string(),
        universe: args.universe,
        since: args.since,
        malformed_git_lines: file_churn.malformed_lines,
        entries,
    };

    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}
