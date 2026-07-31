INSERT INTO orders (id, customer_reference, lines, status, created_at, updated_at, revision, deleted_at)
VALUES (
    '00000000-0000-0000-0000-000000000002'::uuid,
    'MINCO-TEST-ORDER',
    '[{"sku":"MINCO-TEST-SKU","quantity":1}]'::jsonb,
    'accepted',
    '2026-01-01T00:00:00Z'::timestamptz,
    '2026-01-01T00:00:00Z'::timestamptz,
    1,
    NULL
)
ON CONFLICT (id) DO UPDATE SET
    customer_reference = EXCLUDED.customer_reference,
    lines = EXCLUDED.lines,
    status = EXCLUDED.status,
    created_at = EXCLUDED.created_at,
    updated_at = EXCLUDED.updated_at,
    revision = EXCLUDED.revision,
    deleted_at = EXCLUDED.deleted_at;
