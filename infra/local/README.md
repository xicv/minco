# Graph-derived local infrastructure

The primary local workflow derives and supervises the complete topology:

```bash
cargo minco dev --dry-run --json
cargo minco dev
```

These lower-level diagnostics expose only the Compose projection:

```bash
python3 scripts/dev/topology.py
./scripts/dev/up.sh --dry-run
```

The reference graph selects PostgreSQL and Rustack's SSM/STS services. The
individual scripts remain useful when qualifying one boundary:

```bash
./scripts/dev/up.sh
./scripts/dev/migrate.sh
./scripts/dev/run.sh
```

If port 4566 is already assigned, select another host port without changing the
container endpoint:

```bash
MINCO_RUSTACK_PORT=4567 ./scripts/dev/up.sh
MINCO_RUSTACK_PORT=4567 ./scripts/dev/run.sh
```

`MINCO_LOCAL_DATABASE_URL` can select an isolated PostgreSQL database while
preserving an existing development database. A repository `.env` is loaded
after the derived defaults and therefore remains the explicit operator
override.

Run the isolated Rustack compatibility boundary with:

```bash
./scripts/dev/rustack-smoke.sh
```

The smoke proves real S3, SQS, SSM SecureString and STS operations through
standard AWS endpoint variables. It also loads a SecureString through
`minco-aws-lambda::load_secure_parameter`, proving the real Rust SDK adapter
uses the same endpoint path. Provider-neutral plugin selection does not imply
an AWS provider: future application adapters must declare any additional
Rustack services explicitly.
