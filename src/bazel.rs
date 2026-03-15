use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the current workspace root using `bazel info workspace`.
pub fn find_workspace_root() -> Result<PathBuf, Box<dyn Error>> {
    let output = Command::new("bazel").args(["info", "workspace"]).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("bazel info workspace failed: {}", stderr.trim()).into());
    }

    let root = String::from_utf8(output.stdout)?.trim().to_owned();
    if root.is_empty() {
        return Err("bazel info workspace produced empty output".into());
    }

    Ok(PathBuf::from(root))
}

/// Resolves paths to Bazel labels via a single `bazel query --output=location`.
pub fn query_paths(
    workspace_root: &Path,
    paths: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<HashMap<String, String>, Box<dyn Error>> {
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.as_ref().trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if paths.is_empty() {
        return Ok(HashMap::new());
    }

    let query = format!("set({})", paths.join(" "));
    let mut query_file = tempfile::NamedTempFile::new()?;
    query_file.write_all(query.as_bytes())?;
    query_file.flush()?;

    let output = Command::new("bazel")
        .args([
            "query",
            "--query_file",
            query_file.path().to_str().unwrap(),
            "--output=location",
            "--noimplicit_deps",
            "--notool_deps",
            "--keep_going", // continue even if some paths are not build targets
        ])
        .current_dir(workspace_root)
        .output()?;

    let stdout = String::from_utf8(output.stdout)?;
    let path_to_label = stdout
        .lines()
        .filter_map(|line| parse_location_line(line, workspace_root))
        .collect();

    Ok(path_to_label)
}

fn parse_location_line(line: &str, workspace_root: &Path) -> Option<(String, String)> {
    let (path_line_col, desc) = line.trim().rsplit_once(": ")?;
    let label = desc.split_whitespace().next_back()?.to_owned();

    // Path may contain ':' (e.g. Windows), so strip the last two segments.
    let mut parts = path_line_col.rsplitn(3, ':');
    parts.next()?;
    parts.next()?;
    let path = Path::new(parts.next()?.trim());

    let rel = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    Some((rel, label))
}
