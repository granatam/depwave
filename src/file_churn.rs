use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub struct FileChurn {
    pub churn: HashMap<String, u64>,
    pub malformed_lines: u64,
}

/// Computes file churn (file touch frequency) from `git log --name-status`.
///
/// Deleted files are intentionally excluded from the result.
pub fn parse_git_log(workspace_root: &Path, since: Option<&str>) -> Result<FileChurn> {
    let mut cmd = Command::new("git");
    cmd.current_dir(workspace_root).args([
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
    let output = cmd
        .output()
        .context("failed to run `git log --name-status`")?;
    if !output.status.success() {
        bail!(
            "git log failed (status: {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("git log --name-status produced non-UTF-8 output")?;
    Ok(parse_name_status_stdout(&stdout))
}

/// Parses `git log --name-status` stdout into file churn.
fn parse_name_status_stdout(stdout: &str) -> FileChurn {
    let mut churn: HashMap<String, u64> = HashMap::new();
    let mut malformed_lines = 0u64;

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
    malformed_lines: &mut u64,
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
    malformed_lines: &mut u64,
) -> Option<(&'a str, &'a str)> {
    match (fields.next(), fields.next()) {
        (Some(first), Some(second)) => Some((first, second)),
        _ => {
            *malformed_lines += 1;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_status_counts_added_and_modified_files() {
        let parsed = parse_name_status_stdout("A\tsrc/a.rs\nM\tsrc/a.rs\nM\tsrc/b.rs\n");

        assert_eq!(parsed.churn.get("src/a.rs"), Some(&2));
        assert_eq!(parsed.churn.get("src/b.rs"), Some(&1));
        assert_eq!(parsed.malformed_lines, 0);
    }

    #[test]
    fn parse_name_status_rename_carries_previous_churn_to_new_path() {
        let parsed = parse_name_status_stdout(
            "A\tsrc/old.rs\nM\tsrc/old.rs\nR100\tsrc/old.rs\tsrc/new.rs\nM\tsrc/new.rs\n",
        );

        assert_eq!(parsed.churn.get("src/old.rs"), None);
        assert_eq!(parsed.churn.get("src/new.rs"), Some(&4));
        assert_eq!(parsed.malformed_lines, 0);
    }

    #[test]
    fn parse_name_status_copy_counts_new_path_only() {
        let parsed =
            parse_name_status_stdout("A\tsrc/original.rs\nC100\tsrc/original.rs\tsrc/copy.rs\n");

        assert_eq!(parsed.churn.get("src/original.rs"), Some(&1));
        assert_eq!(parsed.churn.get("src/copy.rs"), Some(&1));
        assert_eq!(parsed.malformed_lines, 0);
    }

    #[test]
    fn parse_name_status_delete_removes_file_from_current_result() {
        let parsed =
            parse_name_status_stdout("A\tsrc/deleted.rs\nM\tsrc/deleted.rs\nD\tsrc/deleted.rs\n");

        assert_eq!(parsed.churn.get("src/deleted.rs"), None);
        assert_eq!(parsed.malformed_lines, 0);
    }

    #[test]
    fn parse_name_status_reports_malformed_and_unknown_lines() {
        let parsed = parse_name_status_stdout("M\nX\tsrc/weird.rs\nA\tsrc/ok.rs\n");

        assert_eq!(parsed.churn.get("src/ok.rs"), Some(&1));
        assert_eq!(parsed.malformed_lines, 2);
    }

    #[test]
    fn parse_name_status_ignores_empty_lines() {
        let parsed = parse_name_status_stdout("\n\nA\tsrc/a.rs\n\n");

        assert_eq!(parsed.churn.get("src/a.rs"), Some(&1));
        assert_eq!(parsed.malformed_lines, 0);
    }
}
