# `minco-plugin-ticketing`

`minco-plugin-ticketing` is a disabled-by-default beta plugin for project-scoped
support tickets, requester and agent conversations, internal notes, queues,
assignment, priority, explicit status transitions, verified attachments,
external-message deduplication and deterministic AI context.

The plugin serves a browser-safe bootstrap and the reviewed support launcher.
Trusted applications issue short-lived handoffs through the private
`ticketing.integrate` operation. The portal receives the opaque bearer in a URL
fragment, clears it, and sends it only in `X-Minco-Ticketing-Handoff`. Memory and
SQLite stores consume the digest and create the authoritative result atomically.

Ticketing requires statically selected health, identity, object-storage,
notifications, audit and events plugins. It consumes no global locator and adds
no portal hosting, fixed compute, mailbox, queue, worker, schedule, browser
extension or cloud provider configuration.

## SQLite

The SQLite profile owns a separate Ticketing database. Applications run its
explicit release migration and inject the resulting pool:

```rust,no_run
# async fn example(pool: sqlx::SqlitePool) {
let plugin = minco_plugin_ticketing::TicketingPlugin::sqlite(pool);
# let _ = plugin;
# }
```

Do not share an application's database file or create cross-database foreign
keys. Ticketing transactions include ticket mutations, messages, external
identity deduplication, one-time handoff completion, and local activity intents.

## Integration boundary

See `docs/how-to/ticketing-entry-surfaces.md` for Laravel BFF, portal, CSP,
Manifest V3 and external-mail adapter boundaries. SES and Microsoft Graph remain
separate adapters; the plugin accepts only normalized, verified identities and
never threads messages by subject alone.
