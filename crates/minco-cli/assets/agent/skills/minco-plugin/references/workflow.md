# Plugin checklist

Confirm all applicable contracts:

- stable plugin ID and compatibility version;
- typed constructors and services;
- explicit required/provided capabilities;
- strict configuration schema with secret references, never values;
- operations and authorization implications;
- migrations and classified seeds;
- health and failure behavior;
- resource, IAM, dependency, wake-source, cost, and retention declarations;
- archive inclusion and documentation links; and
- graph, injection, selection, deterministic-order, adapter, and conformance
  tests.

DynamoDB ports remain access-pattern-specific. Do not make a plugin emulate
relational SQL semantics through a generic repository.
