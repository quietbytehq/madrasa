-- Staff logins managed from the Staff screen: a password reset is its own audit action.
-- Guardians and students can carry a family id (in `custom`) from an imported register, so
-- siblings arrive linked; the index serves the lookup during import.

ALTER TABLE audit_log DROP CONSTRAINT IF EXISTS audit_log_action_check;
ALTER TABLE audit_log ADD CONSTRAINT audit_log_action_check
    CHECK (action IN ('create', 'update', 'delete', 'login', 'login_failed', 'lockout', 'logout',
                      'backup', 'restore', 'backup_password_failed', 'late_fee', 'password_reset'));

CREATE INDEX guardians_family_id_idx ON guardians (school_id, (custom ->> 'family_id'))
    WHERE deleted_at IS NULL AND custom ? 'family_id';
