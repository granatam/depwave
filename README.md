# Depwave

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

## Limitations

Depwave currently analyzes files that can be resolved through Bazel. Files such
as `.gitignore`, CI configuration, or other non-Bazel-visible files may appear
in Git history but will be counted as unresolved.

The current score is a rough heuristic. It does not yet account for:

- dependency type
- target build time
- test execution cost
- remote cache hit rate
- other things
