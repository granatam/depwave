use crate::file_churn::FileChurn;
use crate::files::{AnalysisStatus, FileKind, classify_file};

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct Report {
    pub workspace: String,
    pub universe: String,
    pub since: Option<String>,
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
