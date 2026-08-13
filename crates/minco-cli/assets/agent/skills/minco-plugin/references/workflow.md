# Plugin checklist

Release skill freshness is checked against the current Minco changelog and
versioned documentation before the bundle ships.

Confirm all applicable contracts:

- stable plugin ID and compatibility version;
- typed constructors and services;
- explicit required/provided capabilities;
- strict configuration schema with secret references, never values;
- operations and authorization implications;
- migrations and classified seeds;
- health and failure behavior;
- resource, IAM, dependency, wake-source, cost, and retention declarations;
- verified direct upload or rich mail trust, completion, delivery, ambiguity,
  content-safety and cleanup boundaries when applicable;
- archive inclusion and documentation links; and
- graph, injection, selection, deterministic-order, adapter, and conformance
  tests.

DynamoDB ports remain access-pattern-specific. Do not make a plugin emulate
relational SQL semantics through a generic repository.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, use only a plugin's own typed fake for
application tests; do not introduce a generic mock facade or select the fake in
production composition.

At the 1.6 durable audit ledger boundary, keep the V2 record schema-agnostic,
relationship projections bounded, the ledger physically separate, and archive
or retention scheduling outside implicit plugin behavior.

At the 1.7 Apple Container default boundary, declare the local dependency
runtime explicitly. Keep Compose-only plugins on the Docker fallback instead
of inferring that arbitrary Compose behavior can run through Apple Container.
