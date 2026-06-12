use std::collections::{HashMap, HashSet, VecDeque};
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

/// Counts the transitive reverse dependencies of each label via a single
/// `bazel query rdeps(//..., set(...)) --output=graph` call.
///
/// The DOT graph edges are "dependant -> dependency", so for each label we
/// traverse the graph backwards (following predecessors) to find all nodes
/// that transitively depend on it.
pub fn query_rdeps_counts(
    workspace_root: &Path,
    universe: &str,
    labels: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<HashMap<String, usize>, Box<dyn Error>> {
    let universe = universe.trim();
    if universe.is_empty() {
        return Err("`--universe` value should not be empty".into());
    }

    let labels: Vec<String> = labels
        .into_iter()
        .map(|l| l.as_ref().trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if labels.is_empty() {
        return Ok(HashMap::new());
    }

    let query = format!("rdeps({}, set({}))", universe, labels.join(" "));
    let mut query_file = tempfile::NamedTempFile::new()?;
    query_file.write_all(query.as_bytes())?;
    query_file.flush()?;

    let output = Command::new("bazel")
        .args([
            "query",
            "--query_file",
            query_file.path().to_str().unwrap(),
            "--output=graph",
            "--noimplicit_deps",
            "--notool_deps",
        ])
        .current_dir(workspace_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("bazel query rdeps failed: {}", stderr.trim()).into());
    }

    let dot = String::from_utf8(output.stdout)?;
    let predecessors = parse_predecessors_from_dot(&dot)?;
    let appeared_labels = collect_appeared_labels(&predecessors);
    let counts = labels
        .iter()
        .filter(|label| appeared_labels.contains(label.as_str()))
        .map(|label| {
            (
                label.clone(),
                count_transitive_rdeps(label.as_str(), &predecessors),
            )
        })
        .collect();

    Ok(counts)
}

fn parse_predecessors_from_dot(dot: &str) -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {
    use dot_parser::{ast, canonical};

    let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
    if dot.trim().is_empty() {
        return Ok(predecessors);
    }

    let ast_graph = ast::Graph::try_from(dot)
        .map_err(|e| format!("failed to parse bazel --output=graph DOT: {e:?}"))?;
    let graph = canonical::Graph::from(ast_graph);

    // Bazel can collapse several labels into one DOT node separated by '\n'.
    for edge in graph.edges.set {
        let from_labels = split_dot_node_labels(edge.from.as_str());
        let to_labels = split_dot_node_labels(edge.to.as_str());
        for to_label in to_labels {
            predecessors
                .entry(to_label)
                .or_default()
                .extend(from_labels.iter().cloned());
        }
    }

    Ok(predecessors)
}

fn collect_appeared_labels(predecessors: &HashMap<String, Vec<String>>) -> HashSet<String> {
    let mut appeared = HashSet::new();
    for (to, froms) in predecessors {
        appeared.insert(to.clone());
        appeared.extend(froms.iter().cloned());
    }
    appeared
}

fn count_transitive_rdeps<'a>(
    label: &'a str,
    predecessors: &'a HashMap<String, Vec<String>>,
) -> usize {
    let mut visited: HashSet<&str> = HashSet::from([label]);
    let mut queue: VecDeque<&str> = VecDeque::new();

    if let Some(preds) = predecessors.get(label) {
        for pred in preds.iter().map(String::as_str) {
            if visited.insert(pred) {
                queue.push_back(pred);
            }
        }
    }

    while let Some(node) = queue.pop_front() {
        if let Some(preds) = predecessors.get(node) {
            for pred in preds.iter().map(String::as_str) {
                if visited.insert(pred) {
                    queue.push_back(pred);
                }
            }
        }
    }

    visited.len().saturating_sub(1)
}

fn split_dot_node_labels(node: &str) -> Vec<String> {
    // Depending on DOT parsing, collapsed node IDs may contain either actual
    // newline characters or the escaped sequence "\n".
    strip_dot_quotes(node)
        .replace("\\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Strips a single layer of surrounding double-quote characters from a DOT node identifier string.
fn strip_dot_quotes(s: &str) -> &str {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
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
