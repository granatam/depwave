# Understanding Depwave metrics

Depwave currently combines two signals:

- `churn`: how often changed source files owned by a target appear in Git history
- `fanout`: how many targets depend on that owner target

In JSON output, fanout is reported as `transitive_dependents`.

## Churn / Fanout Quadrant

|                | Low fanout        | High fanout        |
| -------------- | ----------------- | ------------------ |
| **Low churn**  | Stable local code | Shared stable code |
| **High churn** | Local churn       | CI risk hotspot    |

> Compare scores only between runs with the same `--universe` and `--since`.

## Patterns

### Broad owner targets

#### Monolithic library

Report:

```json
{
  "target_label": "//pkg:lib",
  "churn": 120,
  "transitive_dependents": 200,
  "source_files": [
    { "path": "pkg/stable_types.rs", "churn": 4 },
    { "path": "pkg/stable_config.rs", "churn": 6 },
    { "path": "pkg/feature_a.rs", "churn": 55 },
    { "path": "pkg/feature_b.rs", "churn": 45 },
    { "path": "pkg/internal_impl.rs", "churn": 10 }
  ]
}
```

Target:

```python
rust_library(
    name = "lib",
    srcs = [
        "stable_types.rs",
        "stable_config.rs",
        "feature_a.rs",
        "feature_b.rs",
        "internal_impl.rs",
    ],
)
```

If most dependents use only part of this target, split by dependency need:

```python
rust_library(
    name = "base",
    srcs = [
        "stable_types.rs",
        "stable_config.rs",
    ],
)

rust_library(
    name = "feature_a",
    srcs = ["feature_a.rs"],
    deps = [":base"],
)

...
```

Common causes:

* one broad target owns unrelated features
* stable shared types are mixed with volatile implementation
* `glob(["*.rs"])` pulls independent files into one target
* source files were grouped by directory rather than dependency need

#### Generated or schema sources

Typical shape:

```python
some_codegen_rule(
    name = "generated_api",
    srcs = ["api.schema"],
)
```

When one schema or generated-source input defines independent APIs, split by API boundary:

```python
some_codegen_rule(
    name = "generated_api_base",
    srcs = ["base.schema"],
)

some_codegen_rule(
    name = "generated_api_feature_a",
    srcs = ["feature_a.schema"],
    deps = [":generated_api_base"],
)

...
```

## Current limitations

Depwave currently ranks dependency blast radius, not CI cost.

The score can miss expensive CI problems when the number of affected targets is
small but those targets are expensive.

Examples not captured well by the current score:
- a low-fanout target used by a few very slow tests or targets with many actions
- a small number of flaky tests with many retries
- non-hermetic actions that repeatedly miss remote cache
- generated outputs that change even when inputs are effectively the same
- broad CI policies that run large test suites for unrelated changes
- test targets with large runtime data or expensive setup

These issues need additional signals such as test runtime, action count, cache
hit rate, flakiness, or CI policy data.
