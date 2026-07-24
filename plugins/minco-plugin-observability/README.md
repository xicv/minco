# minco-plugin-observability

Official structured-observability plugin for Minco.

It supplies typed tracing configuration for compact or CloudWatch-compatible
JSON logging. Applications decide when to initialize the global subscriber,
keeping side effects out of plugin registration.
