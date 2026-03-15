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

    let churn_map = file_churn::parse_git_log(args.since.as_deref())?;

    // Filter out non-target files.
    let path_to_label = bazel::query_paths(&workspace, churn_map.keys())?;

    let mut rows: Vec<_> = path_to_label
        .iter()
        .map(|(path, label)| {
            (
                label.as_str(),
                path.as_str(),
                churn_map.get(path).copied().unwrap_or(0),
            )
        })
        .collect();
    rows.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.cmp(b.0))
            .then_with(|| a.1.cmp(b.1))
    });

    println!("label\tpath\tfrequency");
    for (label, path, frequency) in rows {
        println!("{label}\t{path}\t{frequency}");
    }

    Ok(())
}
