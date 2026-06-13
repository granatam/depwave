use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Report {
    pub workspace: String,
    pub universe: String,
    pub since: Option<String>,
    pub malformed_git_lines: usize,
    pub entries: Vec<TargetImpact>,
}

#[derive(Debug, Serialize)]
pub struct TargetImpact {
    pub source_path: String,
    pub target_label: String,
    pub churn: usize,
    pub dependents: usize,
    pub impact_score: usize,
}
