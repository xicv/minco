CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY,
    customer_reference TEXT NOT NULL CHECK (length(customer_reference) BETWEEN 1 AND 64),
    lines JSONB NOT NULL CHECK (jsonb_typeof(lines) = 'array'),
    status TEXT NOT NULL CHECK (status IN ('accepted')),
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS order_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (length(idempotency_key) BETWEEN 1 AND 200),
    request_fingerprint TEXT NOT NULL,
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS orders_created_at_idx ON orders (created_at DESC, id DESC);
