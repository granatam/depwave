use std::collections::HashMap;
use std::error::Error;
use std::process::Command;

pub struct FileChurn {
    pub churn: HashMap<String, usize>,
    pub malformed_lines: usize,
}

/// Computes file churn (file touch frequency) from `git log --name-status`.
///
/// Deleted files are intentionally excluded from the result.
pub fn parse_git_log(since: Option<&str>) -> Result<FileChurn, Box<dyn Error>> {
    let mut cmd = Command::new("git");
    cmd.args([
        "log",
        "--reverse",
        "-M",
        // TODO: Switch to `-z` for safer parsing of unusual filenames.
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
            "git log failed (status: {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(parse_name_status_stdout(&stdout))
}

/// Parses `git log --name-status` stdout into file churn.
fn parse_name_status_stdout(stdout: &str) -> FileChurn {
    let mut churn: HashMap<String, usize> = HashMap::new();
    let mut malformed_lines = 0usize;

    for raw_line in stdout.lines() {
        if raw_line.trim().is_empty() {
            continue;
        }

        let mut fields = raw_line.splitn(3, '\t');
        let status = match fields.next().and_then(|s| s.chars().next()) {
            Some(status) => status,
            None => {
                malformed_lines += 1;
                continue;
            }
        };

        match status {
            'A' | 'M' => {
                if let Some(path) = take_path(&mut fields, &mut malformed_lines) {
                    *churn.entry(path.to_string()).or_default() += 1;
                }
            }
            'C' => {
                if let Some((_, new_path)) = take_path_pair(&mut fields, &mut malformed_lines) {
                    *churn.entry(new_path.to_string()).or_default() += 1;
                }
            }
            'D' => {
                if let Some(path) = take_path(&mut fields, &mut malformed_lines) {
                    churn.remove(path);
                }
            }
            'R' => {
                if let Some((old_path, new_path)) =
                    take_path_pair(&mut fields, &mut malformed_lines)
                {
                    let previous = churn.remove(old_path).unwrap_or(0);
                    *churn.entry(new_path.to_string()).or_default() += previous + 1;
                }
            }
            _ => {
                malformed_lines += 1;
            }
        }
    }

    FileChurn {
        churn,
        malformed_lines,
    }
}

fn take_path<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    malformed_lines: &mut usize,
) -> Option<&'a str> {
    match fields.next() {
        Some(path) => Some(path),
        None => {
            *malformed_lines += 1;
            None
        }
    }
}

fn take_path_pair<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    malformed_lines: &mut usize,
) -> Option<(&'a str, &'a str)> {
    match (fields.next(), fields.next()) {
        (Some(first), Some(second)) => Some((first, second)),
        _ => {
            *malformed_lines += 1;
            None
        }
    }
}
