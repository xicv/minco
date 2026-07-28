SELECT EXISTS (
    SELECT 1
    FROM orders
    WHERE id = '00000000-0000-0000-0000-000000000001'
      AND customer_reference = 'MINCO-DEMO-ORDER'
      AND status = 'accepted'
);
