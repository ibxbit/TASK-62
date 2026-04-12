-- Ensure test-user cleanup can delete users referenced by transactions.
-- Keep transaction history while nulling actor references when a user is removed.
ALTER TABLE payments.transactions
    DROP CONSTRAINT IF EXISTS transactions_collected_by_fkey;

ALTER TABLE payments.transactions
    ADD CONSTRAINT transactions_collected_by_fkey
    FOREIGN KEY (collected_by)
    REFERENCES auth.users(id)
    ON DELETE SET NULL;
