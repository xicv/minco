ALTER TABLE orders
ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1);

ALTER TABLE orders
ADD COLUMN updated_at TEXT;

ALTER TABLE orders
ADD COLUMN deleted_at TEXT;

UPDATE orders
SET updated_at = created_at
WHERE updated_at IS NULL;

CREATE INDEX IF NOT EXISTS orders_active_created_at_idx
ON orders (created_at DESC, id DESC)
WHERE deleted_at IS NULL;

CREATE TABLE order_idempotency_next (
    idempotency_key TEXT PRIMARY KEY CHECK (length(idempotency_key) BETWEEN 1 AND 200),
    request_fingerprint TEXT NOT NULL,
    order_id TEXT NOT NULL REFERENCES orders(id) ON DELETE RESTRICT,
    response_snapshot TEXT NOT NULL CHECK (json_valid(response_snapshot)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

INSERT INTO order_idempotency_next (
    idempotency_key,
    request_fingerprint,
    order_id,
    response_snapshot,
    created_at
)
SELECT
    replay.idempotency_key,
    replay.request_fingerprint,
    replay.order_id,
    json_object(
        'id', source.id,
        'customer_reference', source.customer_reference,
        'lines', json(source.lines),
        'status', source.status,
        'created_at', source.created_at,
        'updated_at', source.updated_at,
        'revision', source.revision
    ),
    replay.created_at
FROM order_idempotency AS replay
JOIN orders AS source ON source.id = replay.order_id;

DROP TABLE order_idempotency;
ALTER TABLE order_idempotency_next RENAME TO order_idempotency;
