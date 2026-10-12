-- Platform payout date. paid_at stays the client's confirmation of settlement.
ALTER TABLE orders ADD COLUMN received_at TEXT;

-- Orders confirmed before this migration count as paid out, so old orders do not
-- all show as awaiting a payout.
UPDATE orders SET received_at = substr(paid_at, 1, 10)
 WHERE paid_at IS NOT NULL AND status IN ('paid', 'archived');
