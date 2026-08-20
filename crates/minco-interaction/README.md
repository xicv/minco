# minco-interaction

`minco-interaction` contains optional, provider-neutral interaction primitives
shared by Minco plugins and applications. It deliberately is not a plugin and
does not select infrastructure.

The crate provides bounded support-entry values, attachment validation and
object-storage delegation, optional transcription adapters, a tiny static
transition helper, and an explicitly post-commit best-effort activity recorder.
Applications retain ownership of authorization, persistence transactions,
plugin selection, HTTP policy, and deployment.
