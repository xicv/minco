SELECT EXISTS (
    SELECT 1
    FROM orders
    WHERE id = '00000000-0000-0000-0000-000000000002'
      AND customer_reference = 'MINCO-TEST-ORDER'
      AND status = 'accepted'
);
