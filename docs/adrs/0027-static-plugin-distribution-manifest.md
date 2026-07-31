# ADR 0027: Archive-visible static plugin distribution manifests

## Status

Accepted

## Context

Minco plugins are ordinary Rust crates selected and linked at compile time.
Their runtime `PluginDescriptor` describes the constructor that is actually
registered, but inspecting that descriptor requires compiling and constructing
plugin code. The central catalog previously exposed only selection labels and
silently ignored some declared coordinates. A crates.io consumer therefore
could not assess core compatibility, data sensitivity, runtime/database
support, infrastructure implications, or conformance evidence before linking
the crate.

Runtime package scanning, dynamic libraries, and a hosted registry would weaken
Minco's compile-time graph and add an operational service that is unnecessary
for distribution.

## Decision

Every catalog component points from `[package.metadata.minco]` to one strict,
package-root `minco-plugin.json` record. `package.include` explicitly places the
record in the published crate archive. The record uses schema version 1 and
describes:

- component identity, kind, plugin contract version, Minco core compatibility,
  stability, default selection, and Cargo feature;
- supported runtimes and databases, plugin/capability dependencies, provided
  capabilities, configuration schema, operations, and sensitive header names;
- migration and classified seed sets;
- resource intent, conditional Cargo feature, IAM actions, wake sources,
  dependencies, and idle-cost class;
- health checks, data classes, retention and failure behavior;
- tutorial, how-to, reference and explanation links plus inert conformance
  evidence labels.

Authority is intentionally split:

| Data | Authority | Checked overlap |
|---|---|---|
| crate name/version, Cargo dependency and feature graph, files shipped | `Cargo.toml` | catalog crate/path and explicit inclusion of the distribution record |
| pre-link compatibility and the union of supported distribution behavior | `minco-plugin.json` | catalog selection coordinates and the linked descriptor fields below |
| actual statically linked constructor and its configured contributions | runtime `PluginDescriptor` | identity, plugin contract/core versions, stability/default, plugin and capability dependencies, configuration, operations, health, sensitivity and documentation; active migrations/resources must be present in the distribution union |
| application enablement, provider choice, secret references and values | application composition/configuration | not copied into distribution metadata |

`cargo minco plugin list` reads Cargo/package metadata and JSON only; it neither
loads nor constructs plugin code. `cargo minco plugin validate` additionally
compares official statically linked descriptors to the archive record. Evidence
strings are displayed but never executed.

Local package paths must be normalized and resolve inside the project. The
metadata pointer names one regular package-root JSON file, cannot traverse
directories or follow a file symlink, and is size bounded. Unknown fields fail
parsing. Secret configuration fields may declare a name and type but never a
default value. Provider credentials and secret values have no field in this
schema.

## Consequences

- A registry or downloaded `.crate` archive can be assessed without executing
  plugin code.
- Catalog drift, packaging omissions and linked-descriptor drift fail
  deterministically in the existing validation command.
- Conditional adapters can publish a conservative union of supported
  resources while runtime descriptors remain authoritative for the explicitly
  selected instance.
- Adding a plugin remains an ordinary Cargo dependency plus an explicit typed
  constructor registration. Metadata alone never enables or loads code.
- Schema evolution requires a new supported schema version and compatibility
  policy; unknown versions fail closed.

## Rejected alternatives

- Runtime filesystem or registry discovery: breaks the static graph and makes
  deployment contents less reviewable.
- Dynamic library loading or a stable Rust ABI facade: adds compatibility and
  security boundaries without serving Minco's narrow AWS-native goal.
- Treating the central catalog as the complete manifest: it is not shipped in
  each crate archive and cannot describe third-party-style crates independently.
- Embedding secret values or provider policy: application configuration and
  deployment profiles remain authoritative for those choices.
