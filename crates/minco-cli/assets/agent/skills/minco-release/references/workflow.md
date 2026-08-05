# Release states

Report these independently:

- source selected and clean;
- local checks passed;
- hosted checks passed for the exact revision;
- artifact and manifest built and digest-verified;
- tag or registry publication completed;
- deployment applied to the named target;
- runtime behavior observed;
- product/reviewer acceptance recorded; and
- rollback and cleanup readiness proven.

Publication is non-atomic. If transport becomes ambiguous, query authoritative
state before retrying. Never place credentials, endpoints, account identifiers,
customer data, or secret values in committed receipts.
