-- M2 stage 1: report runs are audit-logged with action 'report'.
ALTER TABLE audit_log DROP CONSTRAINT IF EXISTS audit_log_action_check;
ALTER TABLE audit_log ADD CONSTRAINT audit_log_action_check
    CHECK (action IN ('create', 'update', 'delete', 'login', 'login_failed', 'lockout', 'logout',
                      'backup', 'restore', 'backup_password_failed', 'late_fee', 'password_reset',
                      'report'));
