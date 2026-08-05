# Inspect a project with the local workbench

The optional Minco workbench renders the same bounded schema-1 `ProjectView`
used by the local read-only MCP server. It is a presentation surface over
declared repository sources, not a project state store, hosted control plane,
deployment controller, or proof that an application is running.

## Validate without writing or listening

Run the check from the project root:

```bash
cargo minco workbench --check --json
```

The report identifies the project and source digest, reports the configured
input and response limits, and derives node, edge, task, and evidence counts.
`read_only: true`, `listening_sockets: 0`, and `writes: 0` describe this command
only. They do not upgrade local verification into hosted, deployed, runtime, or
review evidence.

## Export a deterministic snapshot

Choose one format and a new project-relative destination whose parent already
exists:

```bash
cargo minco workbench export \
  --format json \
  --output target/workbench-json

cargo minco workbench export \
  --format mermaid \
  --output target/workbench-mermaid

cargo minco workbench export \
  --format static \
  --output target/workbench-static
```

The formats contain:

| Format | Files | Purpose |
|---|---|---|
| `json` | `project-view.json` | Complete machine-readable ProjectView snapshot |
| `mermaid` | `project-view.mmd` | Deterministic graph with opaque node IDs and escaped labels |
| `static` | `index.html`, ProjectView JSON, Mermaid, CSS, and JavaScript | Local accessible browser projection |

Every export is create-only. The destination must be normalized, relative to
the canonical project root, absent, and outside all declared canonical inputs.
Every existing destination-parent component must be a real directory rather
than a symlink. Minco retains the verified parent descriptor, creates a private
0700 staging directory exclusively, writes 0600 artifacts through that
descriptor, rechecks parent and staging identities, and installs the complete
directory with an atomic no-clobber rename. A changed component, identity,
staging entry, or destination fails closed. Unsupported safe-installation
primitives also fail closed.

The static directory contains repository metadata from the bounded view. It
contains no credential or secret-value field, but it may still be sensitive in
a private application. Review it before sharing. Opening `index.html` directly
from a `file:` origin is browser-dependent; serve the directory with a trusted
local-only static server when an exported interactive copy is required.

## Serve the live local view

Resolve and pass the project root explicitly. Port zero lets the operating
system choose an available port:

```bash
cargo minco \
  --root /absolute/canonical/project/root \
  --json \
  workbench serve --port 0
```

The first stdout line is a compact startup object containing the exact
`http://127.0.0.1:PORT` origin. Open that origin in a browser and stop the
process with `Ctrl-C` when finished.

The server binds IPv4 loopback directly, accepts only the exact bound `Host`,
rejects a different `Origin`, emits no permissive CORS header, serves only six
fixed routes, sets `Cache-Control: no-store`, and applies restrictive content,
frame, referrer, and MIME-sniffing policies. It has no authentication and does
not defend against a malicious process already running as the local user, so do
not proxy it, bind it publicly, or expose it through a tunnel.

## Read progress and evidence correctly

Task status rows preserve each raw source status. The complete total is a
derived numerator over the displayed denominator; `planned`, `in_progress`,
and any future raw states remain visible rather than being inferred as
complete.

The six evidence lanes are deliberately independent:

- Source records repository content or metadata.
- Local records checks performed on the local machine.
- Hosted records separately controlled CI or qualification.
- Deployment records release or infrastructure application.
- Runtime records observations from the running target.
- Review records product review, acceptance, or UAT.

Evidence in one lane never upgrades another. For example, a hosted check is not
deployment proof, and deployment is not runtime or product-review proof.

The architecture groups are a compact projection of existing ProjectView
nodes. Labels such as `Adapters` and `Plugins` summarize bounded node kinds;
they do not create or freeze a public application adapter API. ADR-0030 keeps
any such freeze gated on evidence from Minco and a separately authorized
first-party application, with M12-T03 and M12-T04 owning that later adoption
and compatibility decision.

## Accessibility and browser behavior

The workbench includes landmarks, a skip link, keyboard-focus styles, arrow-key
navigation for primary views, raw textual status, and exclusive Graph, Tasks,
and Evidence modes on small screens. Export and read-aloud controls remain
explicit user actions.

Read aloud passes the text already displayed in the workbench to the browser's
Web Speech capability. Minco does not call a speech provider, generate audio,
or store voice data. A browser or operating system may implement a selected
voice using its own service, so its privacy policy still applies. If the API is
unavailable, the control is disabled and all information remains readable as
text.

Run the pinned browser journey after changing HTML, CSS, JavaScript, server
headers, or responsive behavior:

```bash
scripts/test/workbench_browser.sh
```

The journey starts an ephemeral loopback server and checks desktop rendering,
keyboard navigation, read-aloud state, JSON download, small-screen mode
switching, six evidence lanes, horizontal overflow, browser errors, and empty
screenshot artifacts. Dependencies are installed from the Feedback plugin's
locked Playwright package. Screenshots are temporary unless
`WORKBENCH_SCREENSHOT_DIR` names an explicit output directory.
