INSERT INTO orders (id, customer_reference, lines, status, created_at, updated_at, revision, deleted_at)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    'MINCO-TEST-ORDER',
    '[{"sku":"MINCO-TEST-SKU","quantity":1}]',
    'accepted',
    '2026-01-01T00:00:00Z',
    '2026-01-01T00:00:00Z',
    1,
    NULL
)
ON CONFLICT (id) DO UPDATE SET
    customer_reference = excluded.customer_reference,
    lines = excluded.lines,
    status = excluded.status,
    created_at = excluded.created_at,
    updated_at = excluded.updated_at,
    revision = excluded.revision,
    deleted_at = excluded.deleted_at;
