#!/usr/bin/env python3
"""Exercise the real SQLite migration and its critical persistence invariants."""
from __future__ import annotations

import json
import sqlite3
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MIGRATION = ROOT / "examples/orders/migrations/sqlite/0001_orders.sql"


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-sqlite-") as directory:
        database = Path(directory) / "orders.db"
        connection = sqlite3.connect(database)
        connection.execute("PRAGMA foreign_keys = ON")
        connection.executescript(MIGRATION.read_text())

        order_id = str(uuid.uuid4())
        lines = json.dumps([{"sku": "SKU-1", "quantity": 2}])
        connection.execute(
            "INSERT INTO orders (id, customer_reference, lines, status, created_at) VALUES (?, ?, ?, ?, ?)",
            (order_id, "PO-42", lines, "accepted", "2026-07-23T00:00:00Z"),
        )
        connection.execute(
            "INSERT INTO order_idempotency (idempotency_key, request_fingerprint, order_id) VALUES (?, ?, ?)",
            ("request-1", "fingerprint-1", order_id),
        )
        connection.commit()

        row = connection.execute(
            "SELECT customer_reference, json_extract(lines, '$[0].sku') FROM orders WHERE id = ?",
            (order_id,),
        ).fetchone()
        assert row == ("PO-42", "SKU-1")

        try:
            connection.execute(
                "INSERT INTO order_idempotency (idempotency_key, request_fingerprint, order_id) VALUES (?, ?, ?)",
                ("request-2", "fingerprint-2", str(uuid.uuid4())),
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("foreign-key enforcement did not reject a missing order")

        try:
            connection.execute(
                "INSERT INTO orders (id, customer_reference, lines, status, created_at) VALUES (?, ?, ?, ?, ?)",
                (str(uuid.uuid4()), "PO-BAD", "not-json", "accepted", "2026-07-23T00:00:00Z"),
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("json_valid constraint did not reject invalid JSON")

        try:
            connection.execute(
                "INSERT INTO order_idempotency (idempotency_key, request_fingerprint, order_id) VALUES (?, ?, ?)",
                ("request-1", "different", order_id),
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("idempotency primary key did not reject duplicate keys")

        connection.close()
    print("SQLite migration and persistence invariants passed.")


if __name__ == "__main__":
    main()
