# Qualify an exact static-site publication

Static assets can share an exact Minco release while remaining private in S3
and publicly served through CloudFront. Publication is bound to the release and
deployment receipts; it never replaces API deployment evidence.

## Features

Enable `plugin-static-site` and the `aws-adapters` static-site capability. The
application must explicitly select its asset root and deployment target.

## Provider assumptions

The checked recipe exercises local CLI and controller contracts. A real plan
requires an exact successful release manifest and deployment receipt for the
same source, artifact, environment, account, role, stack, and alias.

## Cost and wake behavior

S3 objects and retained versions are `storage_only`; CloudFront/S3 requests are
`request_only`; browser requests are the wake source. DNS, certificate, logs,
invalidations, and retained distributions may add cost without application
compute.

Run the local contract proof:

```bash
cargo test --locked -p cargo-minco static_site
```

Operationally, `cargo minco deploy static-site plan` must receive genuine
release/deployment receipts. Apply remains a separately approved AWS mutation,
and hosted verification must compare public bytes and metadata before the old
site is removed.

## Verification

The matrix executes `static-site-contract`.

## Unsupported gates

No S3 bucket, object, CloudFront distribution, invalidation, certificate, DNS
record, or public website is created or changed by this recipe.
