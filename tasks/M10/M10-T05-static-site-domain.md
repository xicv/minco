---
id: M10-T05
title: Complete static-site and custom-domain deployment
milestone: M10
status: planned
priority: high
area: deployment/static-site
depends_on: [M6-T04, M10-T03]
operations: []
owned_paths:
  - plugins/minco-plugin-static-site/**
  - extensions/minco-aws-adapters/**
  - crates/minco-deploy-aws/**
  - infra/aws/**
  - scripts/aws/**
  - docs/deployment/**
  - tasks/M10/M10-T05-static-site-domain.md
checks:
  - cargo test -p minco-plugin-static-site -p minco-aws-adapters -p minco-deploy-aws --all-features --locked
  - cargo minco deploy verify --static-site
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Complete private-object publication, CloudFront OAC, optional custom-domain
inputs, certificate/DNS guards, cache policy, invalidation, and hosted
byte/hash verification through the generic deployment receipt.

## Acceptance

- traversal and content-type/cache behavior remain safe and deterministic;
- uploaded bytes and deployed object hashes match the release;
- certificate region, DNS ownership, distribution, and invalidation are
  explicit guarded stages;
- live CloudFront proof is separately authorised and cost-labelled;
- removal and rollback behavior is documented.

## Non-goals

- a frontend build system;
- public S3 buckets;
- silently creating domains or certificates.
