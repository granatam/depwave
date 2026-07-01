# Depwave

A change to a highly depended-on file can propagate through the build graph
like a **wave**, affecting many targets, tests, and CI jobs.

Depwave is a change-impact analysis CLI tool for Bazel repositories.

> [!WARNING]
> Depwave is under active development. CLI flags, JSON output schema,
> scoring model, and analysis semantics are likely to change between versions.

It helps identify Bazel targets affected by changed source files because they
combine:

- high Git churn across owned source files
- many transitive Bazel dependents

The current scoring model is intentionally simple:

```text
impact_score = sources_churn_sum * transitive_dependents
```

Depwave can help answer questions like:

- Which targets are depended on by many other targets?
- Which high-churn source files explain that target impact?

## Usage

```bash
depwave --workspace . --universe //... --since 2024-01-01 --limit 20
```

Depwave currently writes the report in JSON format to stdout.

For interpreting report fields and choosing follow-up actions, see
[`docs/understanding-metrics.md`](docs/understanding-metrics.md).

## Usage example

The following example was generated on the [Bazel repository](https://github.com/bazelbuild/bazel)
at version 8.5.1, using `//src/main/...` as the analysis universe.

```bash
$ depwave --since 2024-06-14 --universe //src/main/... --limit 2
...  WARN depwave::bazel: bazel query --output=location returned partial results status=exit status: 3

{
  "workspace": "<path_to_workspace>/bazel",
  "universe": "//src/main/...",
  "since": "2024-06-14",
  "total_churned_files": 2798,
  "analyzed_targets": 718,
  "unresolved_source_files_count": 10,
  "no_owner_source_files_count": 939,
  "unsupported_files_count": 369,
  "malformed_git_lines": 0,
  "entries": [
    {
      "target_label": "//src/main/java/com/google/devtools/build/lib/packages:packages",
      "churn": 254,
      "transitive_dependents": 532,
      "impact_score": 135128,
      "source_files": [
        {
          "path": "src/main/java/com/google/devtools/build/lib/packages/AbstractAttributeMapper.java",
          "file_label": "//src/main/java/com/google/devtools/build/lib/packages:AbstractAttributeMapper.java",
          "churn": 2
        },
        {
          "path": "src/main/java/com/google/devtools/build/lib/packages/AggregatingAttributeMapper.java",
          "file_label": "//src/main/java/com/google/devtools/build/lib/packages:AggregatingAttributeMapper.java",
          "churn": 3
        },
        ...
      ]
    },
    {
      "target_label": "//src/main/java/com/google/devtools/build/lib/skyframe/serialization:serialization",
      "churn": 65,
      "transitive_dependents": 701,
      "impact_score": 45565,
      "source_files": [
        {
          "path": "src/main/java/com/google/devtools/build/lib/skyframe/serialization/ArrayProcessor.java",
          "file_label": "//src/main/java/com/google/devtools/build/lib/skyframe/serialization:ArrayProcessor.java",
          "churn": 1
        },
        {
          "path": "src/main/java/com/google/devtools/build/lib/skyframe/serialization/AsyncDeserializationContext.java",
          "file_label": "//src/main/java/com/google/devtools/build/lib/skyframe/serialization:AsyncDeserializationContext.java",
          "churn": 1
        },
        ...
      ]
    }
  ],
  "unresolved_source_files": [
    {
      "path": ".gitattributes",
      "churn": 3
    },
    ...
  ],
  "no_owner_source_files": [
    ...
    {
      "path": ".bazelrc",
      "file_label": "//:.bazelrc",
      "churn": 10
    },
    ...
  ]
}
```

## Limitations

Depwave classifies changed files into source files, BUILD files, .bzl files,
and workspace/module files.

Currently:
- source files resolved through Bazel are mapped to direct owner targets
- report entries are owner targets, with source files kept as evidence
- source-like files that cannot be resolved through Bazel are reported as
  unresolved
- source files with no direct owner target are reported separately
- BUILD, .bzl, WORKSPACE, and MODULE.bazel files are detected but reported as
  unsupported

The current score is a rough heuristic. It does not yet account for:

- dependency type
- triggered actions count
- target build time
- test execution cost
- remote cache hit rate
- other things
