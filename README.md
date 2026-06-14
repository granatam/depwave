# Depwave

A change to a highly depended-on file can propagate through the build graph
like a **wave**, affecting many targets, tests, and CI jobs.

Depwave is a change-impact analysis CLI tool for Bazel repositories.

It helps identify files and Bazel targets that are risky to change because they combine:

- high Git churn
- many transitive Bazel dependents

The current scoring model is intentionally simple:

```text
impact_score = churn * transitive_dependents
```

Depwave can help answer questions like:

- Which targets are depended on by many other targets?
- Which high-churn files are likely to cause large CI/build impact?

## Usage

```bash
depwave --workspace . --universe //... --since 2024-01-01 --limit 20
```

Depwave currently writes the report in JSON format to stdout.

## Usage example

The following example was generated on the [Bazel repository](https://github.com/bazelbuild/bazel)
at version 8.5.1, using `//src/main/...` as the analysis universe.

```bash
$ depwave --since 2024-06-14 --universe //src/main/... --limit 2
bazel query --output=location: some changed files could not be resolved as Bazel targets

{
  "workspace": "<path_to_workspace>/bazel",
  "universe": "//src/main/...",
  "since": "2024-06-14",
  "total_churned_files": 2799,
  "analyzed_files": 2420,
  "unresolved_files": 10,
  "unsupported_files": 369,
  "malformed_git_lines": 0,
  "entries": [
    {
      "source_path": "src/main/java/com/google/devtools/build/lib/packages/Package.java",
      "kind": "source",
      "status": "analyzed",
      "target_label": "//src/main/java/com/google/devtools/build/lib/packages:Package.java",
      "churn": 40,
      "dependents": 535,
      "impact_score": 21400
    },
    {
      "source_path": "src/main/java/com/google/devtools/build/lib/packages/semantics/BuildLanguageOptions.java",
      "kind": "source",
      "status": "analyzed",
      "target_label": "//src/main/java/com/google/devtools/build/lib/packages/semantics:BuildLanguageOptions.java",
      "churn": 33,
      "dependents": 641,
      "impact_score": 21153
    }
  ]
}
```

## Limitations

Depwave classifies changed files into source files, BUILD files, .bzl files,
and workspace/module files.

Currently:
- source files resolved through Bazel are analyzed
- source-like files that cannot be resolved through Bazel are reported as
  unresolved
- BUILD, .bzl, WORKSPACE, and MODULE.bazel files are detected but reported as
  unsupported

The current score is a rough heuristic. It does not yet account for:

- dependency type
- triggered actions count
- target build time
- test execution cost
- remote cache hit rate
- other things
