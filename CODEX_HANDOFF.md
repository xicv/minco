# Minco framework-completion handoff

Date: 2026-07-27
Task: `M9-T01`
Published baseline: `0.3.1`
Starting `main`: `cb6ffd702a65a59a3195caa64c3709a471b4c21f`
Workspace: `/Users/xicao/Projects/minco-task-m9-t01`

## Current boundary

M9-T01 is complete after exact-head review and local qualification. Its
docs-only framework-definition and roadmap change defines Minco as a
contract-to-cloud framework through one five-plane application graph and
records the M9-M12 completion program.

Included:

- product identity and measurable 1.0 criteria;
- accepted golden-path ADR;
- Diátaxis documentation map;
- M9-M12 roadmap/task records;
- proven README inventory and Feedback-stability corrections;
- deterministic roadmap/task/source-manifest regeneration.

Excluded:

- M6-T10 runtime or Plan IR implementation;
- typed configuration, migration, seeding, dev-supervisor, generator,
  deployment-controller, plugin-tooling, MCP, or workbench source;
- AWS, database, registry, tag, release, and product-repository mutations.

## Next boundary

After this change is on `main`, start `M6-T10` in its own JJ workspace from the
merged prerequisite. It is the trigger-aware multi-runtime Plan IR task and
remains a likely `0.4.0` serialized/public boundary.

## Required continuation checks

Before any continuation:

```bash
# Run Git transport commands from the primary colocated checkout.
cd /Users/xicao/Projects/minco
git fetch --all --tags --prune
jj git import

# Then inspect the isolated task workspace.
cd /Users/xicao/Projects/minco-task-m9-t01
cargo minco task show M6-T10 --json
jj log -r 'conflicts()'
```

Revalidate the exact remote `main`, reviewed M9-T01 state, task ownership, disk
capacity, and current primary AWS/Cargo documentation. No prior local or hosted
result automatically qualifies new runtime source.
