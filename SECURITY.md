# Security Policy

## Supported versions

Security fixes are applied to the latest published minor line. Consumers should
use a signed release, pin the exact version selected by their lockfile, and
review its release manifest before deployment. Older minor lines are not
maintained unless a release advisory explicitly says otherwise.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Send a private report
to the repository owner through GitHub's private vulnerability reporting
facility. Include:

- affected revision and component;
- a minimal reproduction or data-flow description;
- realistic impact and prerequisites;
- whether credentials or production data were involved;
- a safe remediation proposal, when known.

Do not include real secrets or customer data. Maintainers should acknowledge a
report, reproduce it in an isolated environment, assign an owner, and publish a
coordinated fix and advisory.

## Security invariants

- Domain and application crates do not depend on HTTP, SQLx, Lambda, or AWS SDKs.
- Public API behavior is defined by the committed OpenAPI contract.
- API errors use stable problem codes and do not expose database/provider detail.
- CORS uses exact origins; credentialed wildcard CORS is rejected.
- Secrets are referenced by name and resolved at runtime; release manifests and
  deployment plans contain no secret values.
- Production migrations are explicit release operations and never run during
  Lambda cold start.
- IAM permissions are scoped to declared capabilities and resources.
- No default deployment profile creates a NAT Gateway, provisioned concurrency,
  fixed compute, or a scheduled database poller.
- Dependency and toolchain updates require review and the complete quality gate.

## Operational disclosure

The archive's `VERIFICATION.md` records which checks actually ran. A static-only
handoff is not equivalent to compiler, dependency, database, Lambda, SAM, or
real-AWS verification.
