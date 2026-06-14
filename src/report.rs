use crate::file_churn::FileChurn;

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
    pub target_label: String,
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
    let mut entries: Vec<_> = path_to_label
        .iter()
        .map(|(path, label)| {
            let churn = file_churn.churn.get(path).copied().unwrap_or(0);
            let dependents = dependents.get(label.as_str()).copied().unwrap_or(0);

            TargetImpact {
                source_path: path.clone(),
                target_label: label.clone(),
                churn,
                dependents,
                impact_score: churn.saturating_mul(dependents),
            }
        })
        .collect();

    entries.sort_by(TargetImpact::cmp_by_impact);

    entries
}
