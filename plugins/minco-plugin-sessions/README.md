# minco-plugin-sessions

Provider-neutral, revocable sessions for Minco applications.

The plugin owns session issuance, token hashing, expiry, lookup, and revocation. HTTP cookie
serialization and identity-provider login flows remain delivery adapters, keeping the application
core independent of Axum, API Gateway, Cognito, or a particular cookie library.

The in-memory store is intended for tests and local development. Production applications inject a
transactional PostgreSQL, DynamoDB, or other durable `SessionStore` implementation.
