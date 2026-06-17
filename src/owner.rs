#![cfg_attr(not(test), allow(dead_code))]

use crate::bazel::BazelDependencyGraph;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceFile {
    pub(crate) path: String,
    pub(crate) file_label: String,
    pub(crate) churn: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnresolvedSourceFile {
    pub(crate) path: String,
    pub(crate) churn: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerChurn {
    pub(crate) label: String,
    pub(crate) churn: u64,
    pub(crate) source_files: Vec<SourceFile>,
}

pub(crate) fn aggregate_owner_churn(
    file_churn: &HashMap<String, u64>,
    labels_by_path: &HashMap<String, String>,
    graph: &BazelDependencyGraph,
) -> (Vec<OwnerChurn>, Vec<UnresolvedSourceFile>, Vec<SourceFile>) {
    let mut owners_by_label: BTreeMap<String, OwnerChurn> = BTreeMap::new();
    let mut unresolved_files = Vec::new();
    let mut no_owner_files = Vec::new();

    let mut paths: Vec<_> = file_churn.keys().map(String::as_str).collect();
    paths.sort_unstable();

    for path in paths {
        let churn = file_churn[path];
        let Some(file_label) = labels_by_path.get(path) else {
            unresolved_files.push(UnresolvedSourceFile {
                path: path.to_string(),
                churn,
            });
            continue;
        };

        let source_file = SourceFile {
            path: path.to_string(),
            file_label: file_label.clone(),
            churn,
        };
        let owner_labels: BTreeSet<_> = graph
            .direct_predecessors(file_label)
            .iter()
            .map(String::as_str)
            .collect();

        if owner_labels.is_empty() {
            no_owner_files.push(source_file);
            continue;
        }

        for owner_label in owner_labels {
            let owner = owners_by_label
                .entry(owner_label.to_string())
                .or_insert_with(|| OwnerChurn {
                    label: owner_label.to_string(),
                    churn: 0,
                    source_files: Vec::new(),
                });
            owner.churn = owner.churn.saturating_add(churn);
            owner.source_files.push(source_file.clone());
        }
    }

    (
        owners_by_label.into_values().collect(),
        unresolved_files,
        no_owner_files,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_churn(entries: &[(&str, u64)]) -> HashMap<String, u64> {
        entries
            .iter()
            .map(|(path, churn)| ((*path).to_string(), *churn))
            .collect()
    }

    fn labels_by_path(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(path, label)| ((*path).to_string(), (*label).to_string()))
            .collect()
    }

    fn graph(dot: &str) -> BazelDependencyGraph {
        BazelDependencyGraph::from_dot(dot).unwrap()
    }

    #[test]
    fn aggregates_one_source_file_owned_by_one_rule() {
        let file_churn = file_churn(&[("pkg/foo.rs", 4)]);
        let labels_by_path = labels_by_path(&[("pkg/foo.rs", "//pkg:foo.rs")]);
        let graph = graph(
            r#"
            digraph mygraph {
              "//pkg:lib" -> "//pkg:foo.rs"
            }
        "#,
        );

        let (owners, unresolved_files, no_owner_files) =
            aggregate_owner_churn(&file_churn, &labels_by_path, &graph);

        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].label, "//pkg:lib");
        assert_eq!(owners[0].churn, 4);
        assert_eq!(owners[0].source_files.len(), 1);
        assert_eq!(owners[0].source_files[0].path, "pkg/foo.rs");
        assert!(unresolved_files.is_empty());
        assert!(no_owner_files.is_empty());
    }

    #[test]
    fn aggregates_multiple_source_files_under_same_owner() {
        let file_churn = file_churn(&[("pkg/bar.rs", 3), ("pkg/foo.rs", 4)]);
        let labels_by_path = labels_by_path(&[
            ("pkg/bar.rs", "//pkg:bar.rs"),
            ("pkg/foo.rs", "//pkg:foo.rs"),
        ]);
        let graph = graph(
            r#"
            digraph mygraph {
              "//pkg:lib" -> "//pkg:foo.rs"
              "//pkg:lib" -> "//pkg:bar.rs"
            }
        "#,
        );

        let (owners, _, _) = aggregate_owner_churn(&file_churn, &labels_by_path, &graph);

        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].label, "//pkg:lib");
        assert_eq!(owners[0].churn, 7);
        assert_eq!(owners[0].source_files.len(), 2);
        assert_eq!(owners[0].source_files[0].path, "pkg/bar.rs");
        assert_eq!(owners[0].source_files[1].path, "pkg/foo.rs");
    }

    #[test]
    fn assigns_full_file_churn_to_each_owner_for_multi_owner_file() {
        let file_churn = file_churn(&[("pkg/foo.rs", 4)]);
        let labels_by_path = labels_by_path(&[("pkg/foo.rs", "//pkg:foo.rs")]);
        let graph = graph(
            r#"
            digraph mygraph {
              "//pkg:lib" -> "//pkg:foo.rs"
              "//pkg:test" -> "//pkg:foo.rs"
            }
        "#,
        );

        let (owners, _, _) = aggregate_owner_churn(&file_churn, &labels_by_path, &graph);

        assert_eq!(owners.len(), 2);
        assert_eq!(owners[0].label, "//pkg:lib");
        assert_eq!(owners[0].churn, 4);
        assert_eq!(owners[1].label, "//pkg:test");
        assert_eq!(owners[1].churn, 4);
    }

    #[test]
    fn records_file_with_no_owner() {
        let file_churn = file_churn(&[("pkg/foo.rs", 4)]);
        let labels_by_path = labels_by_path(&[("pkg/foo.rs", "//pkg:foo.rs")]);
        let graph = BazelDependencyGraph::default();

        let (owners, unresolved_files, no_owner_files) =
            aggregate_owner_churn(&file_churn, &labels_by_path, &graph);

        assert!(owners.is_empty());
        assert!(unresolved_files.is_empty());
        assert_eq!(no_owner_files.len(), 1);
        assert_eq!(no_owner_files[0].path, "pkg/foo.rs");
        assert_eq!(no_owner_files[0].file_label, "//pkg:foo.rs");
        assert_eq!(no_owner_files[0].churn, 4);
    }

    #[test]
    fn records_unresolved_file_without_label() {
        let file_churn = file_churn(&[("pkg/foo.rs", 4)]);
        let labels_by_path = HashMap::new();
        let graph = BazelDependencyGraph::default();

        let (owners, unresolved_files, no_owner_files) =
            aggregate_owner_churn(&file_churn, &labels_by_path, &graph);

        assert!(owners.is_empty());
        assert_eq!(unresolved_files.len(), 1);
        assert_eq!(unresolved_files[0].path, "pkg/foo.rs");
        assert_eq!(unresolved_files[0].churn, 4);
        assert!(no_owner_files.is_empty());
    }

    #[test]
    fn duplicate_graph_edges_do_not_double_count_owner() {
        let file_churn = file_churn(&[("pkg/foo.rs", 4)]);
        let labels_by_path = labels_by_path(&[("pkg/foo.rs", "//pkg:foo.rs")]);
        let graph = graph(
            r#"
            digraph mygraph {
              "//pkg:lib" -> "//pkg:foo.rs"
              "//pkg:lib" -> "//pkg:foo.rs"
            }
        "#,
        );

        let (owners, _, _) = aggregate_owner_churn(&file_churn, &labels_by_path, &graph);

        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].label, "//pkg:lib");
        assert_eq!(owners[0].churn, 4);
        assert_eq!(owners[0].source_files.len(), 1);
    }
}
