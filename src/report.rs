use crate::file_churn::FileChurn;
use crate::files::{FileKind, classify_file};
use crate::owner::{OwnerImpact, SourceFile, UnresolvedSourceFile};

use serde::Serialize;

pub struct ReportConfig {
    pub workspace: String,
    pub universe: String,
    pub since: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    workspace: String,
    universe: String,
    since: Option<String>,
    total_churned_files: u64,
    analyzed_targets: u64,
    unresolved_source_files_count: u64,
    no_owner_source_files_count: u64,
    unsupported_files_count: u64,
    malformed_git_lines: u64,
    entries: Vec<TargetImpact>,
    unresolved_source_files: Vec<UnresolvedSourceFileEntry>,
    no_owner_source_files: Vec<SourceFileEntry>,
    unsupported_files: Vec<UnsupportedFileEntry>,
}

#[derive(Debug, Serialize)]
struct TargetImpact {
    target_label: String,
    churn: u64,
    transitive_dependents: u64,
    impact_score: u64,
    source_files: Vec<SourceFileEntry>,
}

impl TargetImpact {
    fn cmp_by_impact(a: &Self, b: &Self) -> std::cmp::Ordering {
        b.impact_score
            .cmp(&a.impact_score)
            .then_with(|| b.transitive_dependents.cmp(&a.transitive_dependents))
            .then_with(|| b.churn.cmp(&a.churn))
            .then_with(|| a.target_label.cmp(&b.target_label))
    }
}

#[derive(Debug, Serialize)]
struct SourceFileEntry {
    path: String,
    file_label: String,
    churn: u64,
}

#[derive(Debug, Serialize)]
struct UnresolvedSourceFileEntry {
    path: String,
    churn: u64,
}

#[derive(Debug, Serialize)]
struct UnsupportedFileEntry {
    path: String,
    kind: FileKind,
    churn: u64,
}

pub fn build_report(
    config: ReportConfig,
    file_churn: &FileChurn,
    owner_impacts: Vec<OwnerImpact>,
    unresolved_source_files: Vec<UnresolvedSourceFile>,
    no_owner_source_files: Vec<SourceFile>,
) -> Report {
    let mut entries = build_target_entries(owner_impacts);
    let unresolved_source_files = build_unresolved_source_file_entries(unresolved_source_files);
    let no_owner_source_files = build_source_file_entries(no_owner_source_files);
    let unsupported_files = build_unsupported_file_entries(file_churn);

    let total_churned_files = file_churn.churn.len() as u64;
    let analyzed_targets = entries.len() as u64;
    let unresolved_source_files_count = unresolved_source_files.len() as u64;
    let no_owner_source_files_count = no_owner_source_files.len() as u64;
    let unsupported_files_count = unsupported_files.len() as u64;

    if let Some(limit) = config.limit {
        entries.truncate(limit);
    }

    Report {
        workspace: config.workspace,
        universe: config.universe,
        since: config.since,
        total_churned_files,
        analyzed_targets,
        unresolved_source_files_count,
        no_owner_source_files_count,
        unsupported_files_count,
        malformed_git_lines: file_churn.malformed_lines,
        entries,
        unresolved_source_files,
        no_owner_source_files,
        unsupported_files,
    }
}

fn build_target_entries(owner_impacts: Vec<OwnerImpact>) -> Vec<TargetImpact> {
    let mut entries: Vec<_> = owner_impacts
        .into_iter()
        .map(|impact| TargetImpact {
            target_label: impact.label,
            churn: impact.churn,
            transitive_dependents: impact.transitive_dependents,
            impact_score: impact.impact_score,
            source_files: build_source_file_entries(impact.source_files),
        })
        .collect();

    entries.sort_by(TargetImpact::cmp_by_impact);

    entries
}

fn build_source_file_entries(source_files: Vec<SourceFile>) -> Vec<SourceFileEntry> {
    source_files
        .into_iter()
        .map(|source_file| SourceFileEntry {
            path: source_file.path,
            file_label: source_file.file_label,
            churn: source_file.churn,
        })
        .collect()
}

fn build_unresolved_source_file_entries(
    source_files: Vec<UnresolvedSourceFile>,
) -> Vec<UnresolvedSourceFileEntry> {
    source_files
        .into_iter()
        .map(|source_file| UnresolvedSourceFileEntry {
            path: source_file.path,
            churn: source_file.churn,
        })
        .collect()
}

fn build_unsupported_file_entries(file_churn: &FileChurn) -> Vec<UnsupportedFileEntry> {
    let mut entries: Vec<_> = file_churn
        .churn
        .iter()
        .filter_map(|(path, churn)| {
            let kind = classify_file(path);
            if kind == FileKind::Source {
                None
            } else {
                Some(UnsupportedFileEntry {
                    path: path.clone(),
                    kind,
                    churn: *churn,
                })
            }
        })
        .collect();

    entries.sort_by(|a, b| a.path.cmp(&b.path));

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_owner_first_report_with_explicit_file_buckets() {
        let file_churn = FileChurn {
            churn: std::collections::HashMap::from([
                ("src/foo.rs".to_string(), 4),
                ("src/bar.rs".to_string(), 3),
                ("src/missing.rs".to_string(), 2),
                ("src/orphan.rs".to_string(), 1),
                ("src/BUILD.bazel".to_string(), 5),
            ]),
            malformed_lines: 1,
        };
        let owner_impacts = vec![OwnerImpact {
            label: "//src:lib".to_string(),
            churn: 7,
            source_files: vec![
                SourceFile {
                    path: "src/foo.rs".to_string(),
                    file_label: "//src:foo.rs".to_string(),
                    churn: 4,
                },
                SourceFile {
                    path: "src/bar.rs".to_string(),
                    file_label: "//src:bar.rs".to_string(),
                    churn: 3,
                },
            ],
            transitive_dependents: 10,
            impact_score: 70,
        }];
        let unresolved_source_files = vec![UnresolvedSourceFile {
            path: "src/missing.rs".to_string(),
            churn: 2,
        }];
        let no_owner_source_files = vec![SourceFile {
            path: "src/orphan.rs".to_string(),
            file_label: "//src:orphan.rs".to_string(),
            churn: 1,
        }];

        let report = build_report(
            ReportConfig {
                workspace: "/repo".to_string(),
                universe: "//...".to_string(),
                since: None,
                limit: None,
            },
            &file_churn,
            owner_impacts,
            unresolved_source_files,
            no_owner_source_files,
        );

        assert_eq!(report.total_churned_files, 5);
        assert_eq!(report.analyzed_targets, 1);
        assert_eq!(report.unresolved_source_files_count, 1);
        assert_eq!(report.no_owner_source_files_count, 1);
        assert_eq!(report.unsupported_files_count, 1);
        assert_eq!(report.malformed_git_lines, 1);
        assert_eq!(report.entries[0].target_label, "//src:lib");
        assert_eq!(report.entries[0].churn, 7);
        assert_eq!(report.entries[0].transitive_dependents, 10);
        assert_eq!(report.entries[0].impact_score, 70);
        assert_eq!(report.entries[0].source_files.len(), 2);
        assert_eq!(report.unresolved_source_files[0].path, "src/missing.rs");
        assert_eq!(report.no_owner_source_files[0].path, "src/orphan.rs");
        assert_eq!(report.unsupported_files[0].path, "src/BUILD.bazel");
    }
}
