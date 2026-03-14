mod file_churn;

use clap::Parser;
use std::error::Error;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Only consider commits more recent than this value (passed to `git log --since=...`).
    #[arg(short, long)]
    since: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let churn = file_churn::parse_git_log(args.since.as_deref())?;

    for (path, freq) in churn {
        println!("{path} {freq}");
    }

    Ok(())
}
