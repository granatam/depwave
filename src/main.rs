use clap::Parser;
use std::collections::HashMap;
use std::error::Error;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Only consider commits more recent than this value.
    ///
    /// The value is passed directly to `git log --since=...`.
    #[arg(short, long)]
    since: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut cmd = Command::new("git");
    cmd.args([
        "log",
        "--name-only",
        "--pretty=format:",
        "--diff-filter=ARM",
    ]);

    if let Some(since) = args.since.as_deref() {
        cmd.args(["--since", since]);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(format!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut frequencies: HashMap<String, usize> = HashMap::new();

    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .for_each(|path| {
            *frequencies.entry(path.to_owned()).or_insert(0) += 1;
        });

    let mut frequencies_vec: Vec<(String, usize)> = frequencies.into_iter().collect();
    frequencies_vec.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    for (file, freq) in frequencies_vec {
        println!("{file} {freq}");
    }

    Ok(())
}
