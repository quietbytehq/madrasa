-- Automatic late fees: a daily job (and a check at app start) assesses the late fee on every
-- unpaid voucher past its due date. Each assessment is audited with its own action so the log
-- can be searched for them; the partial index serves the daily scan for vouchers still to do.

ALTER TABLE audit_log DROP CONSTRAINT IF EXISTS audit_log_action_check;
ALTER TABLE audit_log ADD CONSTRAINT audit_log_action_check
    CHECK (action IN ('create', 'update', 'delete', 'login', 'login_failed', 'lockout', 'logout',
                      'backup', 'restore', 'backup_password_failed', 'late_fee'));

CREATE INDEX fee_vouchers_late_fee_due_idx ON fee_vouchers (school_id, due_date)
    WHERE late_fee = 0 AND NOT late_fee_waived AND status IN ('unpaid', 'partial') AND deleted_at IS NULL;
