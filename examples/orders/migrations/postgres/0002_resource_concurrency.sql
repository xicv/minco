ALTER TABLE orders
ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision >= 1),
ADD COLUMN updated_at TIMESTAMPTZ,
ADD COLUMN deleted_at TIMESTAMPTZ;

UPDATE orders
SET updated_at = created_at
WHERE updated_at IS NULL;

ALTER TABLE orders
ALTER COLUMN updated_at SET NOT NULL;

CREATE INDEX IF NOT EXISTS orders_active_created_at_idx
ON orders (created_at DESC, id DESC)
WHERE deleted_at IS NULL;

ALTER TABLE order_idempotency
ADD COLUMN response_snapshot JSONB;

UPDATE order_idempotency AS replay
SET response_snapshot = jsonb_build_object(
    'id', source.id,
    'customer_reference', source.customer_reference,
    'lines', source.lines,
    'status', source.status,
    'created_at', source.created_at,
    'updated_at', source.updated_at,
    'revision', source.revision
)
FROM orders AS source
WHERE source.id = replay.order_id;

ALTER TABLE order_idempotency
ALTER COLUMN response_snapshot SET NOT NULL;
