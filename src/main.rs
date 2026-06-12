mod bazel;
mod file_churn;

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
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let workspace = match args.workspace {
        Some(w) => w,
        None => bazel::find_workspace_root()?,
    };

    let file_churn = file_churn::parse_git_log(args.since.as_deref())?;
    if file_churn.malformed_lines > 0 {
        eprintln!(
            "warning: skipped {} malformed or unknown git --name-status lines",
            file_churn.malformed_lines
        );
    }

    // Filter out non-target files.
    let path_to_label = bazel::query_paths(&workspace, file_churn.churn.keys())?;

    // Count transitive reverse dependencies for each resolved label.
    let rdeps_counts = bazel::query_rdeps_counts(&workspace, path_to_label.values())?;

    let mut rows: Vec<_> = path_to_label
        .iter()
        .filter_map(|(path, label)| {
            let rdeps = rdeps_counts.get(label.as_str()).copied()?;
            Some((
                label.as_str(),
                path.as_str(),
                file_churn.churn.get(path).copied().unwrap_or(0),
                rdeps,
            ))
        })
        .collect();
    rows.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.cmp(b.0))
            .then_with(|| a.1.cmp(b.1))
    });

    println!("label\tpath\tfrequency\trdeps");
    for (label, path, frequency, rdeps) in rows {
        println!("{label}\t{path}\t{frequency}\t{rdeps}");
    }

    Ok(())
}
