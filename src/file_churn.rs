use std::collections::HashMap;
use std::error::Error;
use std::process::Command;

pub fn parse_git_log(since: Option<&str>) -> Result<Vec<(String, usize)>, Box<dyn Error>> {
    let mut cmd = Command::new("git");
    cmd.args([
        "log",
        "--reverse",
        "-M",
        "--name-status",
        "--pretty=format:",
        "--diff-filter=ARMDC",
    ]);
    if let Some(s) = since {
        cmd.args(["--since", s]);
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
    Ok(parse_stdout(&stdout))
}

// Parses `git log --name-status` output and compute per-file churn. Ignores files that were deleted.
fn parse_stdout(stdout: &str) -> Vec<(String, usize)> {
    let mut churn: HashMap<String, usize> = HashMap::new();

    for line in stdout.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }

        let mut tabs = line.splitn(3, '\t');
        let status = match tabs.next().and_then(|s| s.chars().next()) {
            Some(c) => c,
            None => continue,
        };

        match status {
            'A' | 'M' => {
                if let Some(p) = tabs.next() {
                    *churn.entry(p.to_string()).or_default() += 1;
                }
            }
            'C' => {
                if let Some(p) = tabs.nth(1) {
                    *churn.entry(p.to_string()).or_default() += 1;
                }
            }
            'D' => {
                if let Some(p) = tabs.next() {
                    churn.remove(p);
                }
            }
            'R' => {
                if let (Some(old), Some(new)) = (tabs.next(), tabs.next()) {
                    let (old, new) = (old.to_string(), new.to_string());
                    let n = churn.remove(&old).unwrap_or(0);
                    *churn.entry(new).or_default() += n + 1;
                }
            }
            other => unreachable!("unexpected change status: {other}"),
        }
    }

    let mut result: Vec<_> = churn.into_iter().collect();
    result.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    result
}
