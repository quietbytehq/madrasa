-- Pilot stage 4: found by the load test (5,000 students, three years of fees).

-- The Data health page asks for voucher lines that have no fee head. There should be none,
-- so this index stays empty or tiny and the question is answered without reading every line.
CREATE INDEX fee_voucher_lines_without_head_idx ON fee_voucher_lines (voucher_id)
    WHERE fee_head_id IS NULL AND deleted_at IS NULL;

-- The defaulters list looks at vouchers that are still open. After a few years those are a
-- few thousand among hundreds of thousands; without this index the first opening of the day
-- read the whole table from the disk.
CREATE INDEX fee_vouchers_open_idx ON fee_vouchers (school_id, due_date)
    WHERE deleted_at IS NULL AND status IN ('unpaid', 'partial', 'carried');
