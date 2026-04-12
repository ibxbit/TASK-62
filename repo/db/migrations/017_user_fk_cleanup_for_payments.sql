-- Keep required actor columns non-null while allowing user cleanup in tests.

ALTER TABLE payments.refunds
    ALTER COLUMN requested_by SET NOT NULL;

ALTER TABLE payments.refunds
    DROP CONSTRAINT IF EXISTS refunds_requested_by_fkey;

ALTER TABLE payments.refunds
    ADD CONSTRAINT refunds_requested_by_fkey
    FOREIGN KEY (requested_by)
    REFERENCES auth.users(id)
    ON DELETE CASCADE;

ALTER TABLE payments.refunds
    DROP CONSTRAINT IF EXISTS refunds_approved_by_fkey;

ALTER TABLE payments.refunds
    ADD CONSTRAINT refunds_approved_by_fkey
    FOREIGN KEY (approved_by)
    REFERENCES auth.users(id)
    ON DELETE SET NULL;

ALTER TABLE payments.statement_imports
    ALTER COLUMN imported_by SET NOT NULL;

ALTER TABLE payments.statement_imports
    DROP CONSTRAINT IF EXISTS statement_imports_imported_by_fkey;

ALTER TABLE payments.statement_imports
    ADD CONSTRAINT statement_imports_imported_by_fkey
    FOREIGN KEY (imported_by)
    REFERENCES auth.users(id)
    ON DELETE CASCADE;
