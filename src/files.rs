use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Source,
    BuildFile,
    BzlFile,
    WorkspaceFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Analyzed,
    Unresolved,
    Unsupported,
}

pub fn classify_file(path: &str) -> FileKind {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");

    match file_name {
        "BUILD" | "BUILD.bazel" => FileKind::BuildFile,
        "WORKSPACE" | "WORKSPACE.bazel" | "MODULE.bazel" => FileKind::WorkspaceFile,
        _ if path.ends_with(".bzl") => FileKind::BzlFile,
        _ => FileKind::Source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_build_files() {
        assert_eq!(classify_file("BUILD"), FileKind::BuildFile);
        assert_eq!(classify_file("BUILD.bazel"), FileKind::BuildFile);
        assert_eq!(classify_file("src/core/BUILD"), FileKind::BuildFile);
        assert_eq!(classify_file("src/core/BUILD.bazel"), FileKind::BuildFile);
    }

    #[test]
    fn classifies_workspace_files() {
        assert_eq!(classify_file("WORKSPACE"), FileKind::WorkspaceFile);
        assert_eq!(classify_file("WORKSPACE.bazel"), FileKind::WorkspaceFile);
        assert_eq!(classify_file("MODULE.bazel"), FileKind::WorkspaceFile);
    }

    #[test]
    fn classifies_bzl_files() {
        assert_eq!(classify_file("tools/rules/defs.bzl"), FileKind::BzlFile);
    }

    #[test]
    fn classifies_regular_sources() {
        assert_eq!(classify_file("src/lib.rs"), FileKind::Source);
        assert_eq!(classify_file("src/main.cc"), FileKind::Source);
        assert_eq!(classify_file(".github/workflows/ci.yml"), FileKind::Source);
    }
}
