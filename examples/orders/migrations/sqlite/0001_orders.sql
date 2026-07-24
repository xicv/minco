PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    customer_reference TEXT NOT NULL CHECK (length(customer_reference) BETWEEN 1 AND 64),
    lines TEXT NOT NULL CHECK (json_valid(lines)),
    status TEXT NOT NULL CHECK (status IN ('accepted')),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS order_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (length(idempotency_key) BETWEEN 1 AND 200),
    request_fingerprint TEXT NOT NULL,
    order_id TEXT NOT NULL REFERENCES orders(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

CREATE INDEX IF NOT EXISTS orders_created_at_idx ON orders (created_at DESC, id DESC);
