use anyhow::{Context, Result, bail};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the current workspace root using `bazel info workspace`.
pub fn find_workspace_root() -> Result<PathBuf> {
    let output = Command::new("bazel")
        .args(["info", "workspace"])
        .output()
        .context("failed to run `bazel info workspace`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("bazel info workspace failed: {}", stderr.trim());
    }

    let root = String::from_utf8(output.stdout)
        .context("bazel info workspace produced non-UTF-8 output")?
        .trim()
        .to_owned();
    if root.is_empty() {
        bail!("bazel info workspace produced empty output");
    }

    Ok(PathBuf::from(root))
}

/// Resolves paths to Bazel labels via a single `bazel query --output=location`.
pub fn resolve_paths_to_labels(
    workspace_root: &Path,
    paths: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<HashMap<String, String>> {
    let paths = collect_unique_nonempty_strings(paths);

    if paths.is_empty() {
        return Ok(HashMap::new());
    }

    let query = format!("set({})", paths.join(" "));
    let mut query_file =
        tempfile::NamedTempFile::new().context("failed to create temporary Bazel query file")?;
    query_file
        .write_all(query.as_bytes())
        .context("failed to write Bazel path query file")?;
    query_file
        .flush()
        .context("failed to flush Bazel path query file")?;

    let output = Command::new("bazel")
        .arg("query")
        .arg("--query_file")
        .arg(query_file.path())
        .args([
            "--output=location",
            "--noimplicit_deps",
            "--notool_deps",
            "--keep_going", // continue even if some paths are not build targets
        ])
        .current_dir(workspace_root)
        .output()
        .context("failed to run `bazel query --output=location`")?;

    if !output.status.success() {
        eprintln!(
            "bazel query --output=location: some changed files could not be resolved as Bazel targets"
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("bazel query --output=location produced non-UTF-8 output")?;
    let labels_by_path = stdout
        .lines()
        .filter_map(|line| parse_location_line(line, workspace_root))
        .collect();

    Ok(labels_by_path)
}

/// Counts transitive dependents of each label via a single `bazel query` call.
pub fn count_transitive_dependents_by_label(
    workspace_root: &Path,
    universe: &str,
    labels: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<HashMap<String, u64>> {
    let universe = universe.trim();
    if universe.is_empty() {
        bail!("`--universe` value should not be empty");
    }

    let labels = collect_unique_nonempty_strings(labels);

    if labels.is_empty() {
        return Ok(HashMap::new());
    }

    let query = format!("rdeps({}, set({}))", universe, labels.join(" "));
    let mut query_file =
        tempfile::NamedTempFile::new().context("failed to create temporary Bazel query file")?;
    query_file
        .write_all(query.as_bytes())
        .context("failed to write Bazel rdeps query file")?;
    query_file
        .flush()
        .context("failed to flush Bazel rdeps query file")?;

    let output = Command::new("bazel")
        .arg("query")
        .arg("--query_file")
        .arg(query_file.path())
        .args([
            "--output=graph",
            // Bazel graph output is factored by default and intended for
            // visualization. Depwave needs an unfactored graph so every DOT
            // node maps to real labels.
            "--nograph:factored",
            // Prevent Bazel from truncating long DOT node labels.
            "--graph:node_limit=-1",
            "--noimplicit_deps",
            "--notool_deps",
            "--order_output=full",
        ])
        .current_dir(workspace_root)
        .output()
        .context("failed to run `bazel query --output=graph`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("bazel query rdeps failed: {}", stderr.trim());
    }

    let dot = String::from_utf8(output.stdout)
        .context("bazel query --output=graph produced non-UTF-8 output")?;
    let predecessors = parse_predecessors_from_dot(&dot)?;
    let graph_labels = collect_graph_labels(&predecessors);
    let counts = labels
        .iter()
        .filter(|label| graph_labels.contains(label.as_str()))
        .map(|label| {
            (
                label.clone(),
                count_transitive_dependents_for_label(label.as_str(), &predecessors),
            )
        })
        .collect();

    Ok(counts)
}

fn parse_predecessors_from_dot(dot: &str) -> Result<HashMap<String, Vec<String>>> {
    use dot_parser::{ast, canonical};

    let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
    if dot.trim().is_empty() {
        return Ok(predecessors);
    }

    let ast_graph = ast::Graph::try_from(dot)
        .map_err(|e| anyhow::anyhow!("failed to parse bazel --output=graph DOT: {e:?}"))?;
    let graph = canonical::Graph::from(ast_graph);

    for edge in graph.edges.set {
        let from = strip_dot_quotes(edge.from.as_str()).to_owned();
        let to = strip_dot_quotes(edge.to.as_str()).to_owned();

        predecessors.entry(to).or_default().push(from);
    }

    Ok(predecessors)
}

fn collect_graph_labels(predecessors: &HashMap<String, Vec<String>>) -> HashSet<&str> {
    let mut graph_labels = HashSet::new();
    for (to, froms) in predecessors {
        graph_labels.insert(to.as_str());
        graph_labels.extend(froms.iter().map(String::as_str));
    }
    graph_labels
}

fn collect_unique_nonempty_strings(
    values: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.as_ref().trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn count_transitive_dependents_for_label<'a>(
    label: &'a str,
    predecessors: &'a HashMap<String, Vec<String>>,
) -> u64 {
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

    u64::try_from(visited.len())
        .unwrap_or(u64::MAX)
        .saturating_sub(1)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    fn sorted(mut values: Vec<String>) -> Vec<String> {
        values.sort();
        values
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn strip_dot_quotes_removes_surrounding_quotes() {
        assert_eq!(strip_dot_quotes(r#""//pkg:target""#), "//pkg:target");
    }

    #[test]
    fn strip_dot_quotes_leaves_unquoted_string_unchanged() {
        assert_eq!(strip_dot_quotes("//pkg:target"), "//pkg:target");
    }

    #[test]
    fn parse_predecessors_from_dot_returns_empty_map_for_empty_dot() {
        let predecessors = parse_predecessors_from_dot("").unwrap();

        assert!(predecessors.is_empty());
    }

    #[test]
    fn parse_predecessors_from_dot_returns_error_for_invalid_dot() {
        let err = parse_predecessors_from_dot("not dot").unwrap_err();

        assert!(
            err.to_string()
                .contains("failed to parse bazel --output=graph DOT"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_predecessors_from_dot_parses_unfactored_graph() {
        let dot = r#"
            digraph mygraph {
              "//app:bin" -> "//lib:core"
              "//test:unit" -> "//lib:core"
              "//lib:core" -> "//base:base"
            }
        "#;

        let predecessors = parse_predecessors_from_dot(dot).unwrap();

        assert_eq!(
            sorted(predecessors.get("//lib:core").unwrap().clone()),
            sorted(strings(&["//app:bin", "//test:unit"]))
        );

        assert_eq!(
            predecessors.get("//base:base").unwrap(),
            &strings(&["//lib:core"])
        );
    }

    #[test]
    fn parse_predecessors_from_dot_keeps_factored_source_node_as_single_node() {
        let dot = r#"
            digraph mygraph {
              "//app:bin\n//test:unit" -> "//lib:core"
            }
        "#;

        let predecessors = parse_predecessors_from_dot(dot).unwrap();

        let froms = predecessors.get("//lib:core").unwrap();

        assert_eq!(froms.len(), 1);

        assert!(
            froms[0].contains("//app:bin"),
            "factored source node should contain //app:bin, got {:?}",
            froms[0]
        );

        assert!(
            froms[0].contains("//test:unit"),
            "factored source node should contain //test:unit, got {:?}",
            froms[0]
        );
    }

    #[test]
    fn parse_predecessors_from_dot_keeps_factored_destination_node_as_single_node() {
        let dot = r#"
            digraph mygraph {
              "//app:bin" -> "//lib:core\n//lib:util"
            }
        "#;

        let predecessors = parse_predecessors_from_dot(dot).unwrap();

        assert_eq!(predecessors.len(), 1);

        let factored_destination = predecessors.keys().next().unwrap();

        assert!(
            factored_destination.contains("//lib:core"),
            "factored destination node should contain //lib:core, got {factored_destination:?}"
        );

        assert!(
            factored_destination.contains("//lib:util"),
            "factored destination node should contain //lib:util, got {factored_destination:?}"
        );

        assert_eq!(
            predecessors.get(factored_destination).unwrap(),
            &strings(&["//app:bin"])
        );
    }

    #[test]
    fn count_transitive_dependents_for_label_walks_predecessors() {
        let predecessors = HashMap::from([
            (
                "//lib:core".to_string(),
                strings(&["//app:bin", "//test:unit"]),
            ),
            ("//base:base".to_string(), strings(&["//lib:core"])),
        ]);

        assert_eq!(
            count_transitive_dependents_for_label("//base:base", &predecessors),
            3
        );
        assert_eq!(
            count_transitive_dependents_for_label("//lib:core", &predecessors),
            2
        );
        assert_eq!(
            count_transitive_dependents_for_label("//app:bin", &predecessors),
            0
        );
    }

    #[test]
    fn count_transitive_dependents_for_label_handles_cycles_without_infinite_loop() {
        let predecessors = HashMap::from([
            ("//a:a".to_string(), strings(&["//b:b"])),
            ("//b:b".to_string(), strings(&["//a:a", "//c:c"])),
        ]);

        assert_eq!(
            count_transitive_dependents_for_label("//a:a", &predecessors),
            2
        );
    }

    #[test]
    fn collect_graph_labels_includes_sources_and_destinations() {
        let predecessors = HashMap::from([
            (
                "//lib:core".to_string(),
                strings(&["//app:bin", "//test:unit"]),
            ),
            ("//base:base".to_string(), strings(&["//lib:core"])),
        ]);

        let graph_labels = collect_graph_labels(&predecessors);

        assert!(graph_labels.contains("//app:bin"));
        assert!(graph_labels.contains("//test:unit"));
        assert!(graph_labels.contains("//lib:core"));
        assert!(graph_labels.contains("//base:base"));
    }

    #[test]
    fn factored_graph_does_not_make_individual_labels_appear() {
        let dot = r#"
            digraph mygraph {
              "//app:bin\n//test:unit" -> "//lib:core"
              "//lib:core" -> "//base:base"
            }
        "#;

        let predecessors = parse_predecessors_from_dot(dot).unwrap();
        let graph_labels = collect_graph_labels(&predecessors);

        assert!(!graph_labels.contains("//app:bin"));
        assert!(!graph_labels.contains("//test:unit"));

        assert!(graph_labels.contains("//lib:core"));
        assert!(graph_labels.contains("//base:base"));

        assert_eq!(
            count_transitive_dependents_for_label("//base:base", &predecessors),
            2
        );
        assert_eq!(
            count_transitive_dependents_for_label("//lib:core", &predecessors),
            1
        );
        assert_eq!(
            count_transitive_dependents_for_label("//app:bin", &predecessors),
            0
        );
        assert_eq!(
            count_transitive_dependents_for_label("//test:unit", &predecessors),
            0
        );
    }

    #[test]
    fn parse_location_line_returns_workspace_relative_path_and_label() {
        let workspace_root = Path::new("/repo");
        let line = "/repo/src/lib/foo.rs:12:34: source file //src/lib:foo.rs";

        assert_eq!(
            parse_location_line(line, workspace_root),
            Some(("src/lib/foo.rs".to_string(), "//src/lib:foo.rs".to_string()))
        );
    }

    #[test]
    fn parse_location_line_keeps_path_when_outside_workspace() {
        let workspace_root = Path::new("/repo");
        let line = "/tmp/generated/foo.rs:12:34: source file @ext//pkg:foo.rs";

        assert_eq!(
            parse_location_line(line, workspace_root),
            Some((
                "/tmp/generated/foo.rs".to_string(),
                "@ext//pkg:foo.rs".to_string()
            ))
        );
    }

    #[test]
    fn parse_location_line_handles_colons_inside_path() {
        let workspace_root = Path::new("C:/repo");
        let line = "C:/repo/src/lib/foo.rs:12:34: source file //src/lib:foo.rs";

        assert_eq!(
            parse_location_line(line, workspace_root),
            Some(("src/lib/foo.rs".to_string(), "//src/lib:foo.rs".to_string()))
        );
    }

    #[test]
    fn parse_location_line_returns_none_for_malformed_line() {
        let workspace_root = Path::new("/repo");

        assert_eq!(
            parse_location_line("not a bazel location line", workspace_root),
            None
        );
    }
}
