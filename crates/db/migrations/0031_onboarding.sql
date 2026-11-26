-- Pilot onboarding: guardians and the fee structure go through the import engine too.
ALTER TABLE import_batches DROP CONSTRAINT IF EXISTS import_batches_kind_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_kind_check
    CHECK (kind IN ('students', 'staff', 'opening_balances', 'board_results', 'student_subjects',
                    'enquiries', 'guardians', 'fee_structure'));
