# CLI reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `cargo-minco Clap command model`
- `cargo-minco generated --help output`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

The executable is `cargo-minco`; Cargo exposes it as `cargo minco`. Hidden implementation commands are excluded by Clap. Mutation authority still comes from the relevant command's guards and documentation.

## Command tree

- `cargo minco architecture`
- `cargo minco check`
- `cargo minco config`
- `cargo minco contract`
- `cargo minco cost`
- `cargo minco db`
- `cargo minco deploy`
- `cargo minco destroy`
- `cargo minco dev`
- `cargo minco doctor`
- `cargo minco explain`
- `cargo minco feedback`
- `cargo minco inspect`
- `cargo minco make`
- `cargo minco new`
- `cargo minco package`
- `cargo minco perf`
- `cargo minco plugin`
- `cargo minco promote`
- `cargo minco release`
- `cargo minco roadmap`
- `cargo minco rollback`
- `cargo minco stubs`
- `cargo minco task`
- `cargo minco test`
- `cargo minco update`
- `cargo minco upgrade`
- `cargo minco vcs`
  - `cargo minco config check`
  - `cargo minco config diff`
  - `cargo minco config explain`
  - `cargo minco config schema`
  - `cargo minco contract check`
  - `cargo minco contract diff`
  - `cargo minco contract sync`
  - `cargo minco db migrate`
  - `cargo minco db plan`
  - `cargo minco db seed`
  - `cargo minco db status`
  - `cargo minco db verify`
  - `cargo minco deploy apply`
  - `cargo minco deploy changeset`
  - `cargo minco deploy plan`
  - `cargo minco deploy render-sam`
  - `cargo minco deploy review`
  - `cargo minco deploy static-site`
  - `cargo minco deploy verify`
  - `cargo minco feedback attachment`
  - `cargo minco feedback inbox`
  - `cargo minco feedback pull`
  - `cargo minco feedback reply`
  - `cargo minco feedback show`
  - `cargo minco feedback status`
  - `cargo minco make adapter`
  - `cargo minco make migration`
  - `cargo minco make module`
  - `cargo minco make operation`
  - `cargo minco make plugin`
  - `cargo minco make resource`
  - `cargo minco make seeder`
  - `cargo minco make test`
  - `cargo minco make worker`
  - `cargo minco plugin add`
  - `cargo minco plugin disable`
  - `cargo minco plugin doctor`
  - `cargo minco plugin enable`
  - `cargo minco plugin explain`
  - `cargo minco plugin init`
  - `cargo minco plugin list`
  - `cargo minco plugin new`
  - `cargo minco plugin remove`
  - `cargo minco plugin test`
  - `cargo minco plugin validate`
  - `cargo minco release create`
  - `cargo minco release verify`
  - `cargo minco roadmap render`
  - `cargo minco roadmap status`
  - `cargo minco stubs publish`
  - `cargo minco task graph`
  - `cargo minco task list`
  - `cargo minco task next`
  - `cargo minco task ready`
  - `cargo minco task show`
  - `cargo minco task verify`
  - `cargo minco test all`
  - `cargo minco test e2e`
  - `cargo minco test feature`
  - `cargo minco test unit`
  - `cargo minco update apply`
  - `cargo minco update check`
  - `cargo minco upgrade report`
  - `cargo minco vcs init`
  - `cargo minco vcs status`
  - `cargo minco vcs task-finish`
  - `cargo minco vcs task-start`
    - `cargo minco deploy static-site apply`
    - `cargo minco deploy static-site plan`

## Generated help

### `cargo minco`

```text
Contract-first Rust development and deployment control plane

Usage: cargo-minco [OPTIONS] <COMMAND>

Commands:
  new
  doctor
  dev           Run the graph-declared local development topology
  check
  config
  contract
  make
  stubs
  inspect
  explain
  deploy
  destroy       Plan or apply exact, preview-only environment cleanup
  cost
  perf
  architecture
  roadmap
  task
  plugin
  test
  db
  package       Build and seal an exact, independently verifiable release package
  promote       Route live API traffic to an exact successfully verified release
  rollback      Assess an exact older promoted release before routing with `promote`
  release
  update
  upgrade
  vcs
  feedback      Inspect and advance the first-class client feedback loop
  help          Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
  -V, --version      Print version
```

#### `cargo minco architecture`

```text
Usage: cargo-minco architecture [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco check`

```text
Usage: cargo-minco check [OPTIONS]

Options:
      --root <ROOT>
      --with-cargo
      --json
      --with-optional
  -h, --help           Print help
```

#### `cargo minco config`

```text
Usage: cargo-minco config [OPTIONS] <COMMAND>

Commands:
  check    Validate one effective environment and print its deterministic digest
  explain  Explain one field's redacted value and complete override provenance
  diff     Compare two validated environment graphs without exposing secrets
  schema   Print the application and statically linked plugin schema
  help     Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco contract`

```text
Usage: cargo-minco contract [OPTIONS] <COMMAND>

Commands:
  check
  sync
  diff   Compare the current contract with the contract stored at a VCS revision
  help   Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco cost`

```text
Usage: cargo-minco cost [OPTIONS]

Options:
      --config <CONFIG>
      --root <ROOT>
      --json
  -h, --help             Print help
```

#### `cargo minco db`

```text
Usage: cargo-minco db [OPTIONS] <COMMAND>

Commands:
  plan
  status
  verify
  migrate
  seed
  help     Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco deploy`

```text
Usage: cargo-minco deploy [OPTIONS] <COMMAND>

Commands:
  plan
  render-sam
  changeset
  apply
  verify
  review
  static-site
  help         Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco destroy`

```text
Plan or apply exact, preview-only environment cleanup

Usage: cargo-minco destroy [OPTIONS]

Options:
      --root <ROOT>
      --target-config <TARGET_CONFIG>                  [default: infra/aws/deployment-targets.toml]
      --environment <ENVIRONMENT>
      --json
      --review <REVIEW>                                [default: target/minco/review.json]
      --receipt <RECEIPT>                              [default: target/minco/cleanup-receipt.json]
      --approve-review-digest <APPROVE_REVIEW_DIGEST>
      --dry-run
  -h, --help                                           Print help
```

#### `cargo minco dev`

```text
Run the graph-declared local development topology

Usage: cargo-minco dev [OPTIONS]

Options:
      --dry-run
          Print the deterministic development plan without starting anything
      --root <ROOT>

      --environment <ENVIRONMENT>
          Typed runtime configuration environment
      --json

      --profile <PROFILE>
          Named development/deployment profile; defaults to the manifest selection
      --no-migrate
          Do not apply the declared local migration command
      --seed <SEED>
          Explicit local seed profile to apply
      --with-worker <WITH_WORKERS>
          Start a declared worker that is disabled by default
      --without-worker <WITHOUT_WORKERS>
          Omit a declared worker that is enabled by default
      --frontend
          Start the application-defined frontend process
      --no-frontend
          Omit the application-defined frontend process
      --port <PORT>
          Override the local API port
      --rustack-port <RUSTACK_PORT>
          Override the local Rustack port
  -h, --help
          Print help
```

#### `cargo minco doctor`

```text
Usage: cargo-minco doctor [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco explain`

```text
Usage: cargo-minco explain [OPTIONS] <OPERATION_ID>

Arguments:
  <OPERATION_ID>

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco feedback`

```text
Inspect and advance the first-class client feedback loop

Usage: cargo-minco feedback [OPTIONS] --url <URL> --token <TOKEN> <COMMAND>

Commands:
  inbox       List the developer feedback inbox
  show        Show one feedback thread as JSON or AI-ready Markdown
  reply       Reply to the client or add an internal developer note
  status      Move a feedback thread through its explicit workflow
  pull        Materialize an AI-ready feedback file in the repository task area
  attachment  Download a screenshot, voice note, or other attachment
  help        Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --url <URL>      Feedback plugin base URL, ending in `/_minco/feedback` [env: MINCO_FEEDBACK_URL=]
      --json
      --token <TOKEN>  Developer bearer token configured by the Feedback plugin [env: MINCO_FEEDBACK_DEVELOPER_TOKEN]
  -h, --help           Print help
```

#### `cargo minco inspect`

```text
Usage: cargo-minco inspect [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco make`

```text
Usage: cargo-minco make [OPTIONS] <COMMAND>

Commands:
  module     Generate domain and application module boundaries without business rules
  operation  Generate failing application and HTTP specifications for one contract operation
  resource   Generate failing specifications for one reviewed five-action resource contract
  migration  Generate an empty, explicitly classified SQL migration
  seeder     Generate an empty seeder with a fail-closed verification query
  worker     Generate a disabled worker entrypoint and failing specification
  adapter    Generate an infrastructure adapter boundary and failing behavioral specification
  test       Generate only the failing specifications for one contract operation
  plugin     Generate an application-owned statically linked plugin crate
  help       Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco new`

```text
Usage: cargo-minco new [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case application and package prefix

Options:
      --directory <DIRECTORY>  Destination directory; defaults to the application name
      --root <ROOT>
      --database <DATABASE>    Initial persistence runtime and deployment profile [default: postgres] [possible values: postgres, sqlite]
      --json
      --vcs <VCS>              Version-control initialization. JJ is the Minco default [default: jj] [possible values: jj, none]
  -h, --help                   Print help
```

#### `cargo minco package`

```text
Build and seal an exact, independently verifiable release package

Usage: cargo-minco package [OPTIONS]

Options:
      --config <CONFIG>

      --root <ROOT>

      --environment <ENVIRONMENT>

      --json

      --plan <PLAN>
          [default: target/minco/plan.json]
      --template <TEMPLATE>
          [default: target/minco/template.yaml]
      --output <OUTPUT>
          [default: target/minco/release.json]
      --static-site-manifest <STATIC_SITE_MANIFEST>
          [default: target/minco/static-site-release.json]
      --attestation <ATTESTATIONS>
          Repository-relative detached signature or provenance statement
  -h, --help
          Print help
```

#### `cargo minco perf`

```text
Usage: cargo-minco perf [OPTIONS]

Options:
      --config <CONFIG>
      --root <ROOT>
      --json
  -h, --help             Print help
```

#### `cargo minco plugin`

```text
Usage: cargo-minco plugin [OPTIONS] <COMMAND>

Commands:
  list
  add       Add a catalog plugin through Minco's static facade registration
  explain   Explain a plugin's complete archive-visible contract without loading its code
  doctor    Diagnose catalog, compatibility, selection, Cargo, and static registration drift
  init      Adopt an existing local plugin package into the reviewed catalog
  remove    Plan removal and refuse while application behavior or data remains owned by the plugin
  enable
  disable
  new
  validate
  test
  help      Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco promote`

```text
Route live API traffic to an exact successfully verified release

Usage: cargo-minco promote [OPTIONS]

Options:
      --manifest <MANIFEST>
          [default: target/minco/release.json]
      --root <ROOT>

      --json

      --receipt <RECEIPT>
          [default: target/minco/deployment-receipt.json]
      --verification <VERIFICATION>
          [default: target/minco/hosted-verification.json]
      --output <OUTPUT>
          [default: target/minco/promotion-receipt.json]
      --approve-verification-digest <APPROVE_VERIFICATION_DIGEST>

      --dry-run

      --canary
          Plan an opt-in alarm-guarded API alias canary
      --target-config <TARGET_CONFIG>
          [default: infra/aws/deployment-targets.toml]
      --environment <ENVIRONMENT>

      --canary-output <CANARY_OUTPUT>
          [default: target/minco/canary-receipt.json]
  -h, --help
          Print help
```

#### `cargo minco release`

```text
Usage: cargo-minco release [OPTIONS] <COMMAND>

Commands:
  create
  verify
  help    Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco roadmap`

```text
Usage: cargo-minco roadmap [OPTIONS] <COMMAND>

Commands:
  status
  render
  help    Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco rollback`

```text
Assess an exact older promoted release before routing with `promote`

Usage: cargo-minco rollback [OPTIONS]

Options:
      --current-root <CURRENT_ROOT>
          Clean exact-source checkout containing the current promotion evidence
      --root <ROOT>

      --json

      --target-root <TARGET_ROOT>
          Clean exact-source checkout containing the older target promotion evidence
      --current-promotion <CURRENT_PROMOTION>
          [default: target/minco/promotion-receipt.json]
      --target-promotion <TARGET_PROMOTION>
          [default: target/minco/rollback-target-promotion-receipt.json]
      --data-compatibility-evidence <DATA_COMPATIBILITY_EVIDENCE>
          Exact operator-reviewed evidence that the older application can read current data
      --dry-run

  -h, --help
          Print help
```

#### `cargo minco stubs`

```text
Usage: cargo-minco stubs [OPTIONS] <COMMAND>

Commands:
  publish  Publish framework generator stubs into `stubs/minco` for app-owned customization
  help     Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco task`

```text
Usage: cargo-minco task [OPTIONS] <COMMAND>

Commands:
  list
  ready
  next
  show
  graph
  verify
  help    Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco test`

```text
Usage: cargo-minco test [OPTIONS] <COMMAND>

Commands:
  unit
  feature
  e2e
  all
  help     Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco update`

```text
Usage: cargo-minco update [OPTIONS] <COMMAND>

Commands:
  check
  apply
  help   Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco upgrade`

```text
Usage: cargo-minco upgrade [OPTIONS] <COMMAND>

Commands:
  report  Inventory application-facing compatibility boundaries for an upgrade review
  help    Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

#### `cargo minco vcs`

```text
Usage: cargo-minco vcs [OPTIONS] <COMMAND>

Commands:
  init
  status
  task-start
  task-finish
  help         Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco config check`

```text
Validate one effective environment and print its deterministic digest

Usage: cargo-minco config check [OPTIONS]

Options:
      --environment <ENVIRONMENT>  [default: dev]
      --root <ROOT>
      --json
      --set <OVERRIDES>            Highest-precedence typed override in KEY=JSON-or-string form
  -h, --help                       Print help
```

##### `cargo minco config diff`

```text
Compare two validated environment graphs without exposing secrets

Usage: cargo-minco config diff [OPTIONS] --from <FROM> --to <TO>

Options:
      --from <FROM>
      --root <ROOT>
      --json
      --to <TO>
  -h, --help         Print help
```

##### `cargo minco config explain`

```text
Explain one field's redacted value and complete override provenance

Usage: cargo-minco config explain [OPTIONS] <PATH>

Arguments:
  <PATH>

Options:
      --environment <ENVIRONMENT>  [default: dev]
      --root <ROOT>
      --json
      --set <OVERRIDES>            Highest-precedence typed override in KEY=JSON-or-string form
  -h, --help                       Print help
```

##### `cargo minco config schema`

```text
Print the application and statically linked plugin schema

Usage: cargo-minco config schema [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco contract check`

```text
Usage: cargo-minco contract check [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco contract diff`

```text
Compare the current contract with the contract stored at a VCS revision

Usage: cargo-minco contract diff [OPTIONS] --against <AGAINST>

Options:
      --against <AGAINST>
      --root <ROOT>
      --json
  -h, --help               Print help
```

##### `cargo minco contract sync`

```text
Usage: cargo-minco contract sync [OPTIONS]

Options:
      --check
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco db migrate`

```text
Usage: cargo-minco db migrate [OPTIONS] --set <SET> --database-url-env <DATABASE_URL_ENV> --expected-plan-digest <EXPECTED_PLAN_DIGEST> --receipt <RECEIPT>

Options:
      --root <ROOT>

      --set <SET>
          Migration set to apply, including its declared dependency closure
      --database-url-env <DATABASE_URL_ENV>
          Name of the environment variable containing the direct migration database URL
      --json

      --expected-plan-digest <EXPECTED_PLAN_DIGEST>
          Digest emitted by `minco db plan --set <id>`
      --receipt <RECEIPT>
          Durable JSON receipt destination
      --allow-destructive
          Permit plans containing data-rewrite or destructive migrations
  -h, --help
          Print help
```

##### `cargo minco db plan`

```text
Usage: cargo-minco db plan [OPTIONS]

Options:
      --root <ROOT>
      --set <SET>
      --json
  -h, --help         Print help
```

##### `cargo minco db seed`

```text
Usage: cargo-minco db seed [OPTIONS]

Options:
      --profile <PROFILE>
          Seed class to plan or apply: reference, demo, test, or bootstrap
      --root <ROOT>

      --environment <ENVIRONMENT>
          Declared environment class used for the seed allowlist; defaults to local
      --json

      --set <SET>
          Seed set to inspect or apply
      --database-url-env <DATABASE_URL_ENV>
          Name of the environment variable containing the direct seed database URL
      --expected-plan-digest <EXPECTED_PLAN_DIGEST>
          Digest emitted by the matching seed dry-run
      --receipt <RECEIPT>
          Durable JSON receipt destination for an applied seed plan
      --dry-run
          Produce the complete seed plan without connecting or mutating
      --verify
          Verify seed source, or the selected target when a URL environment is provided
      --allow-destructive
          Permit plans containing destructive seed operations
      --authorize-bootstrap <AUTHORIZE_BOOTSTRAP>
          Exact environment acknowledgement required for bootstrap execution
  -h, --help
          Print help
```

##### `cargo minco db status`

```text
Usage: cargo-minco db status [OPTIONS]

Options:
      --root <ROOT>

      --set <SET>
          Migration set to inspect. Omitting this and the URL environment lists source state only
      --database-url-env <DATABASE_URL_ENV>
          Name of the environment variable containing the database URL
      --json

  -h, --help
          Print help
```

##### `cargo minco db verify`

```text
Usage: cargo-minco db verify [OPTIONS]

Options:
      --root <ROOT>

      --set <SET>
          Migration set to inspect. Omitting this and the URL environment lists source state only
      --database-url-env <DATABASE_URL_ENV>
          Name of the environment variable containing the database URL
      --json

  -h, --help
          Print help
```

##### `cargo minco deploy apply`

```text
Usage: cargo-minco deploy apply [OPTIONS]

Options:
      --changeset <CHANGESET>
          [default: target/minco/change-set.json]
      --root <ROOT>

      --json

      --migration-plan <MIGRATION_PLAN>
          [default: target/minco/migration-plan.json]
      --migration-receipt <MIGRATION_RECEIPT>
          [default: target/minco/migration-receipt.json]
      --receipt <RECEIPT>
          [default: target/minco/deployment-receipt.json]
      --approve-changeset-digest <APPROVE_CHANGESET_DIGEST>

      --dry-run

  -h, --help
          Print help
```

##### `cargo minco deploy changeset`

```text
Usage: cargo-minco deploy changeset [OPTIONS]

Options:
      --root <ROOT>

      --target-config <TARGET_CONFIG>
          [default: infra/aws/deployment-targets.toml]
      --environment <ENVIRONMENT>

      --json

      --manifest <MANIFEST>
          [default: target/minco/release.json]
      --output <OUTPUT>
          [default: target/minco/change-set.json]
      --approve-release-digest <APPROVE_RELEASE_DIGEST>

      --dry-run

  -h, --help
          Print help
```

##### `cargo minco deploy plan`

```text
Usage: cargo-minco deploy plan [OPTIONS]

Options:
      --config <CONFIG>
      --root <ROOT>
      --environment <ENVIRONMENT>
      --json
      --target-config <TARGET_CONFIG>  [default: infra/aws/deployment-targets.toml]
      --output <OUTPUT>
      --stdout
  -h, --help                           Print help
```

##### `cargo minco deploy render-sam`

```text
Usage: cargo-minco deploy render-sam [OPTIONS]

Options:
      --config <CONFIG>
      --root <ROOT>
      --json
      --output <OUTPUT>  [default: infra/aws/generated/template.yaml]
  -h, --help             Print help
```

##### `cargo minco deploy review`

```text
Usage: cargo-minco deploy review [OPTIONS]

Options:
      --root <ROOT>
      --target-config <TARGET_CONFIG>            [default: infra/aws/deployment-targets.toml]
      --environment <ENVIRONMENT>
      --json
      --manifest <MANIFEST>                      [default: target/minco/release.json]
      --deployment-receipt <DEPLOYMENT_RECEIPT>  [default: target/minco/deployment-receipt.json]
      --feedback <FEEDBACK>
      --delivery-trace <DELIVERY_TRACE>
      --output <OUTPUT>                          [default: target/minco/review.json]
      --dry-run
  -h, --help                                     Print help
```

##### `cargo minco deploy static-site`

```text
Usage: cargo-minco deploy static-site [OPTIONS] <COMMAND>

Commands:
  plan
  apply
  help   Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco deploy verify`

```text
Usage: cargo-minco deploy verify [OPTIONS]

Options:
      --manifest <MANIFEST>
          [default: target/minco/release.json]
      --root <ROOT>

      --json

      --receipt <RECEIPT>
          [default: target/minco/deployment-receipt.json]
      --output <OUTPUT>
          [default: target/minco/hosted-verification.json]
      --static-site

      --static-site-publication <STATIC_SITE_PUBLICATION>
          [default: target/minco/static-site-publication.json]
      --static-site-output <STATIC_SITE_OUTPUT>
          [default: target/minco/static-site-verification.json]
      --dry-run

  -h, --help
          Print help
```

##### `cargo minco feedback attachment`

```text
Download a screenshot, voice note, or other attachment

Usage: cargo-minco feedback --url <URL> --token <TOKEN> attachment [OPTIONS] --output <OUTPUT> <ID> <ATTACHMENT_ID>

Arguments:
  <ID>
  <ATTACHMENT_ID>

Options:
      --output <OUTPUT>
      --root <ROOT>
      --json
  -h, --help             Print help
```

##### `cargo minco feedback inbox`

```text
List the developer feedback inbox

Usage: cargo-minco feedback --url <URL> --token <TOKEN> inbox [OPTIONS]

Options:
      --root <ROOT>
      --status <STATUS>
      --json
      --limit <LIMIT>    [default: 50]
  -h, --help             Print help
```

##### `cargo minco feedback pull`

```text
Materialize an AI-ready feedback file in the repository task area

Usage: cargo-minco feedback --url <URL> --token <TOKEN> pull [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --output <OUTPUT>
      --root <ROOT>
      --json
  -h, --help             Print help
```

##### `cargo minco feedback reply`

```text
Reply to the client or add an internal developer note

Usage: cargo-minco feedback --url <URL> --token <TOKEN> reply [OPTIONS] --body <BODY> <ID>

Arguments:
  <ID>

Options:
      --body <BODY>
      --root <ROOT>
      --internal
      --json
      --author <AUTHOR>
  -h, --help             Print help
```

##### `cargo minco feedback show`

```text
Show one feedback thread as JSON or AI-ready Markdown

Usage: cargo-minco feedback --url <URL> --token <TOKEN> show [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --format <FORMAT>  [default: markdown] [possible values: json, markdown]
      --root <ROOT>
      --json
  -h, --help             Print help
```

##### `cargo minco feedback status`

```text
Move a feedback thread through its explicit workflow

Usage: cargo-minco feedback --url <URL> --token <TOKEN> status [OPTIONS] <ID> <STATUS>

Arguments:
  <ID>
  <STATUS>

Options:
      --resolution <RESOLUTION>
      --root <ROOT>
      --author <AUTHOR>
      --json
  -h, --help                     Print help
```

##### `cargo minco make adapter`

```text
Generate an infrastructure adapter boundary and failing behavioral specification

Usage: cargo-minco make adapter [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make migration`

```text
Generate an empty, explicitly classified SQL migration

Usage: cargo-minco make migration [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make module`

```text
Generate domain and application module boundaries without business rules

Usage: cargo-minco make module [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make operation`

```text
Generate failing application and HTTP specifications for one contract operation

Usage: cargo-minco make operation [OPTIONS] <OPERATION_ID>

Arguments:
  <OPERATION_ID>  Existing lowerCamelCase `OpenAPI` operationId

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make plugin`

```text
Generate an application-owned statically linked plugin crate

Usage: cargo-minco make plugin [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make resource`

```text
Generate failing specifications for one reviewed five-action resource contract

Usage: cargo-minco make resource [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make seeder`

```text
Generate an empty seeder with a fail-closed verification query

Usage: cargo-minco make seeder [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make test`

```text
Generate only the failing specifications for one contract operation

Usage: cargo-minco make test [OPTIONS] <OPERATION_ID>

Arguments:
  <OPERATION_ID>  Existing lowerCamelCase `OpenAPI` operationId

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco make worker`

```text
Generate a disabled worker entrypoint and failing specification

Usage: cargo-minco make worker [OPTIONS] <NAME>

Arguments:
  <NAME>  Lower-kebab-case generator name

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin add`

```text
Add a catalog plugin through Minco's static facade registration

Usage: cargo-minco plugin add [OPTIONS] <PLUGIN>

Arguments:
  <PLUGIN>  Stable plugin ID or catalog crate name

Options:
      --dry-run      Print the complete deterministic plan without changing files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin disable`

```text
Usage: cargo-minco plugin disable [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --dry-run
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin doctor`

```text
Diagnose catalog, compatibility, selection, Cargo, and static registration drift

Usage: cargo-minco plugin doctor [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin enable`

```text
Usage: cargo-minco plugin enable [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --dry-run
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin explain`

```text
Explain a plugin's complete archive-visible contract without loading its code

Usage: cargo-minco plugin explain [OPTIONS] <PLUGIN>

Arguments:
  <PLUGIN>  Stable plugin ID or catalog crate name

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin init`

```text
Adopt an existing local plugin package into the reviewed catalog

Usage: cargo-minco plugin init [OPTIONS] <PATH>

Arguments:
  <PATH>  Project-relative local plugin package directory

Options:
      --dry-run      Print the complete deterministic plan without changing files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin list`

```text
Usage: cargo-minco plugin list [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin new`

```text
Usage: cargo-minco plugin new [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --dry-run
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin remove`

```text
Plan removal and refuse while application behavior or data remains owned by the plugin

Usage: cargo-minco plugin remove [OPTIONS] <PLUGIN>

Arguments:
  <PLUGIN>  Stable plugin ID or catalog crate name

Options:
      --dry-run      Print the complete deterministic plan and blockers without changing files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin test`

```text
Usage: cargo-minco plugin test [OPTIONS] [PLUGIN]

Arguments:
  [PLUGIN]  Stable plugin ID or catalog crate name

Options:
      --all          Test every local catalog component with the public offline conformance kit
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco plugin validate`

```text
Usage: cargo-minco plugin validate [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco release create`

```text
Usage: cargo-minco release create [OPTIONS] --artifact <ARTIFACT>

Options:
      --artifact <ARTIFACT>
      --root <ROOT>
      --json
      --plan <PLAN>          [default: infra/aws/generated/plan.json]
      --template <TEMPLATE>  [default: infra/aws/generated/template.yaml]
      --output <OUTPUT>      [default: target/minco/release.json]
  -h, --help                 Print help
```

##### `cargo minco release verify`

```text
Usage: cargo-minco release verify [OPTIONS] <MANIFEST>

Arguments:
  <MANIFEST>

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco roadmap render`

```text
Usage: cargo-minco roadmap render [OPTIONS]

Options:
      --format <FORMAT>  [default: mermaid] [possible values: mermaid, json]
      --root <ROOT>
      --json
      --output <OUTPUT>
  -h, --help             Print help
```

##### `cargo minco roadmap status`

```text
Usage: cargo-minco roadmap status [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco stubs publish`

```text
Publish framework generator stubs into `stubs/minco` for app-owned customization

Usage: cargo-minco stubs publish [OPTIONS]

Options:
      --dry-run      Print the deterministic edit plan without changing application files
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco task graph`

```text
Usage: cargo-minco task graph [OPTIONS]

Options:
      --output <OUTPUT>
      --root <ROOT>
      --json
  -h, --help             Print help
```

##### `cargo minco task list`

```text
Usage: cargo-minco task list [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco task next`

```text
Usage: cargo-minco task next [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco task ready`

```text
Usage: cargo-minco task ready [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco task show`

```text
Usage: cargo-minco task show [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco task verify`

```text
Usage: cargo-minco task verify [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco test all`

```text
Usage: cargo-minco test all [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco test e2e`

```text
Usage: cargo-minco test e2e [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco test feature`

```text
Usage: cargo-minco test feature [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco test unit`

```text
Usage: cargo-minco test unit [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco update apply`

```text
Usage: cargo-minco update apply [OPTIONS]

Options:
      --root <ROOT>
      --yes
      --json
      --toolchain
      --dependencies
      --run-checks
  -h, --help          Print help
```

##### `cargo minco update check`

```text
Usage: cargo-minco update check [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco upgrade report`

```text
Inventory application-facing compatibility boundaries for an upgrade review

Usage: cargo-minco upgrade report [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco vcs init`

```text
Usage: cargo-minco vcs init [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco vcs status`

```text
Usage: cargo-minco vcs status [OPTIONS]

Options:
      --root <ROOT>
      --json
  -h, --help         Print help
```

##### `cargo minco vcs task-finish`

```text
Usage: cargo-minco vcs task-finish [OPTIONS] --message <MESSAGE> <ID>

Arguments:
  <ID>

Options:
      --message <MESSAGE>
      --root <ROOT>
      --json
      --push
  -h, --help               Print help
```

##### `cargo minco vcs task-start`

```text
Usage: cargo-minco vcs task-start [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --destination <DESTINATION>
      --root <ROOT>
      --json
  -h, --help                       Print help
```

###### `cargo minco deploy static-site apply`

```text
Usage: cargo-minco deploy static-site apply [OPTIONS] --approve-release-digest <APPROVE_RELEASE_DIGEST>

Options:
      --root <ROOT>

      --target-config <TARGET_CONFIG>
          [default: infra/aws/deployment-targets.toml]
      --environment <ENVIRONMENT>

      --json

      --manifest <MANIFEST>
          [default: target/minco/release.json]
      --deployment-receipt <DEPLOYMENT_RECEIPT>
          [default: target/minco/deployment-receipt.json]
      --output <OUTPUT>
          [default: target/minco/static-site-publication.json]
      --approve-release-digest <APPROVE_RELEASE_DIGEST>

  -h, --help
          Print help
```

###### `cargo minco deploy static-site plan`

```text
Usage: cargo-minco deploy static-site plan [OPTIONS]

Options:
      --root <ROOT>

      --target-config <TARGET_CONFIG>
          [default: infra/aws/deployment-targets.toml]
      --environment <ENVIRONMENT>

      --json

      --manifest <MANIFEST>
          [default: target/minco/release.json]
      --deployment-receipt <DEPLOYMENT_RECEIPT>
          [default: target/minco/deployment-receipt.json]
      --output <OUTPUT>
          [default: target/minco/static-site-publication.json]
  -h, --help
          Print help
```
