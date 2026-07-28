INSERT INTO orders (id, customer_reference, lines, status, created_at)
VALUES (
    '00000000-0000-0000-0000-000000000001'::uuid,
    'MINCO-DEMO-ORDER',
    '[{"sku":"MINCO-DEMO-SKU","quantity":1}]'::jsonb,
    'accepted',
    '2026-01-01T00:00:00Z'::timestamptz
)
ON CONFLICT (id) DO UPDATE SET
    customer_reference = EXCLUDED.customer_reference,
    lines = EXCLUDED.lines,
    status = EXCLUDED.status,
    created_at = EXCLUDED.created_at;
