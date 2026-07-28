# Compatibility reports

Minco exposes two deterministic, read-only reports:

```text
cargo minco contract diff --against <revision> --json
cargo minco upgrade report --json
```

They provide evidence for an upgrade review. Neither command proves semantic
business behavior, deployment safety, persisted-data compatibility or
rollback safety.

## Contract diff

`contract diff` validates the current contract and the contract stored at the
requested JJ or Git revision. It reads the historical file through the VCS and
does not check out or modify the working copy. Revisions are bounded to simple
names, commit IDs and ancestry expressions; option-like or shell-shaped input
is rejected.

Both inputs use Minco's constrained OpenAPI loader. Local `#/...` references
are resolved recursively with cycle protection. External or unresolved
references are reported as `uncertain`; they are never silently accepted.

The schema-1 JSON report includes the two source identifiers and SHA-256
digests, an aggregate `classification`, sorted operation/schema changes,
evidence for every change and explicit limitations. Aggregation uses this
precedence:

```text
breaking > uncertain > non_breaking
```

`non_breaking` only means that the bounded classifier found no breaking or
uncertain structural change. It is not a behavioral compatibility guarantee.
The command succeeds after producing a valid report even when its
classification is `breaking`; automation must inspect `classification`.

### Bounded classifications

| Change | Classification |
|---|---|
| Add/remove operation | non-breaking / breaking |
| Change operation method or path | breaking |
| Require/remove authentication | breaking / uncertain |
| Require/remove the idempotency contract | breaking / breaking |
| Add/remove component schema | non-breaking / breaking |
| Add/remove a type constraint | breaking / uncertain |
| Change an existing type | breaking |
| Add/remove an enum constraint | breaking / uncertain |
| Add/remove an enum value | uncertain / breaking |
| Add optional property | non-breaking |
| Add required property or remove property | breaking |
| Remove required marker | uncertain |
| Change a recognized validation constraint | uncertain |
| Change unclassified operation/schema structure | uncertain |

Descriptions, summaries, examples, tags and other documentation-only operation
fields are ignored. Request/response direction can change the meaning of
otherwise similar schema edits, so the classifier uses `uncertain` where a
direction-independent answer would overstate evidence.

## Application upgrade report

`upgrade report` inventories the application boundaries consumed by release
notes and migration guides:

- application and CLI Rust minimum versions;
- running CLI version and the application's declared Minco requirement;
- selected Cargo features and default-feature policy;
- configuration field names, kinds and secret/required flags;
- plugin catalog schema, selections and linked descriptor versions;
- manifest, deployment-plan and OpenAPI schema/version identifiers;
- stable diagnostics and report limitations.

The command reads `minco.toml` as versioned data before strict manifest
loading. An unsupported manifest schema therefore produces a stable warning
and the remaining available evidence instead of preventing the report itself.
Project-declared files must still resolve to ordinary files inside the project.

Configuration defaults and values are excluded. Secret-reference names and
secret values are never serialized. The overall assessment remains
`review_required`: the report is an inventory against the running CLI, not a
replacement for release notes, compilation, application tests or runtime
verification.

## Upgrade workflow

1. Run `cargo minco upgrade report --json` before changing the Minco version.
2. Save the report with the release-review evidence.
3. Update the exact dependency and selected feature set.
4. Run the report again and compare schema-1 fields and diagnostic codes.
5. Run `cargo minco contract diff --against <reviewed-revision> --json`.
6. Review every `breaking` and `uncertain` item against request/response use.
7. Run contract, compiler, application, adapter and deployment-plan checks.
8. Treat migrations, live deployment and promotion as separate approvals.
