use crate::file_churn::FileChurn;
use crate::files::{AnalysisStatus, FileKind, classify_file};

use serde::Serialize;
use std::collections::HashMap;

pub struct ReportConfig {
    pub workspace: String,
    pub universe: String,
    pub since: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub workspace: String,
    pub universe: String,
    pub since: Option<String>,
    pub total_churned_files: u64,
    pub analyzed_files: u64,
    pub unresolved_files: u64,
    pub unsupported_files: u64,
    pub malformed_git_lines: u64,
    pub entries: Vec<TargetImpact>,
}

#[derive(Debug, Serialize)]
pub struct TargetImpact {
    pub source_path: String,
    pub kind: FileKind,
    pub status: AnalysisStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    pub churn: u64,
    pub dependents: u64,
    pub impact_score: u64,
}

impl TargetImpact {
    pub fn cmp_by_impact(a: &Self, b: &Self) -> std::cmp::Ordering {
        b.impact_score
            .cmp(&a.impact_score)
            .then_with(|| b.dependents.cmp(&a.dependents))
            .then_with(|| b.churn.cmp(&a.churn))
            .then_with(|| a.source_path.cmp(&b.source_path))
            .then_with(|| a.target_label.cmp(&b.target_label))
    }
}

pub fn build_report(
    config: ReportConfig,
    file_churn: &FileChurn,
    path_to_label: &HashMap<String, String>,
    dependents: &HashMap<String, u64>,
) -> Report {
    let mut entries = build_report_entries(file_churn, path_to_label, dependents);

    let total_churned_files = file_churn.churn.len() as u64;
    let analyzed_files = count_status(&entries, AnalysisStatus::Analyzed);
    let unresolved_files = count_status(&entries, AnalysisStatus::Unresolved);
    let unsupported_files = count_status(&entries, AnalysisStatus::Unsupported);

    if let Some(limit) = config.limit {
        entries.truncate(limit);
    }

    Report {
        workspace: config.workspace,
        universe: config.universe,
        since: config.since,
        malformed_git_lines: file_churn.malformed_lines,
        total_churned_files,
        analyzed_files,
        unresolved_files,
        unsupported_files,
        entries,
    }
}

pub fn build_report_entries(
    file_churn: &FileChurn,
    path_to_label: &HashMap<String, String>,
    dependents: &HashMap<String, u64>,
) -> Vec<TargetImpact> {
    let mut entries: Vec<_> = file_churn
        .churn
        .iter()
        .map(|(path, churn)| build_entry(path, *churn, path_to_label, dependents))
        .collect();

    entries.sort_by(TargetImpact::cmp_by_impact);

    entries
}

fn build_entry(
    path: &str,
    churn: u64,
    path_to_label: &HashMap<String, String>,
    dependents: &HashMap<String, u64>,
) -> TargetImpact {
    let kind = classify_file(path);
    match kind {
        FileKind::Source => {
            if let Some(label) = path_to_label.get(path) {
                let dependents = dependents.get(label.as_str()).copied().unwrap_or(0);
                TargetImpact {
                    source_path: path.to_string(),
                    kind,
                    status: AnalysisStatus::Analyzed,
                    target_label: Some(label.clone()),
                    churn,
                    dependents,
                    impact_score: churn.saturating_mul(dependents),
                }
            } else {
                TargetImpact {
                    source_path: path.to_string(),
                    kind,
                    status: AnalysisStatus::Unresolved,
                    target_label: None,
                    churn,
                    dependents: 0,
                    impact_score: 0,
                }
            }
        }
        FileKind::BuildFile => unsupported_entry(path, kind, churn),
        FileKind::BzlFile => unsupported_entry(path, kind, churn),
        FileKind::WorkspaceFile => unsupported_entry(path, kind, churn),
    }
}

fn unsupported_entry(path: &str, kind: FileKind, churn: u64) -> TargetImpact {
    TargetImpact {
        source_path: path.to_string(),
        kind,
        status: AnalysisStatus::Unsupported,
        target_label: None,
        churn,
        dependents: 0,
        impact_score: 0,
    }
}

pub fn count_status(entries: &[TargetImpact], status: AnalysisStatus) -> u64 {
    entries
        .iter()
        .filter(|entry| entry.status == status)
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_entries_for_analyzed_unresolved_and_unsupported_files() {
        let file_churn = FileChurn {
            churn: HashMap::from([
                ("src/lib.rs".to_string(), 3),
                ("README.md".to_string(), 2),
                ("src/BUILD.bazel".to_string(), 4),
                ("tools/defs.bzl".to_string(), 5),
                ("MODULE.bazel".to_string(), 1),
            ]),
            malformed_lines: 0,
        };

        let path_to_label = HashMap::from([("src/lib.rs".to_string(), "//src:lib.rs".to_string())]);

        let dependents = HashMap::from([("//src:lib.rs".to_string(), 10)]);

        let entries = build_report_entries(&file_churn, &path_to_label, &dependents);

        assert_eq!(count_status(&entries, AnalysisStatus::Analyzed), 1);
        assert_eq!(count_status(&entries, AnalysisStatus::Unresolved), 1);
        assert_eq!(count_status(&entries, AnalysisStatus::Unsupported), 3);

        let analyzed = entries
            .iter()
            .find(|entry| entry.source_path == "src/lib.rs")
            .unwrap();

        assert_eq!(analyzed.status, AnalysisStatus::Analyzed);
        assert_eq!(analyzed.kind, FileKind::Source);
        assert_eq!(analyzed.target_label.as_deref(), Some("//src:lib.rs"));
        assert_eq!(analyzed.churn, 3);
        assert_eq!(analyzed.dependents, 10);
        assert_eq!(analyzed.impact_score, 30);

        let build_file = entries
            .iter()
            .find(|entry| entry.source_path == "src/BUILD.bazel")
            .unwrap();

        assert_eq!(build_file.kind, FileKind::BuildFile);
        assert_eq!(build_file.status, AnalysisStatus::Unsupported);
        assert_eq!(build_file.target_label, None);
        assert_eq!(build_file.impact_score, 0);
    }
}
