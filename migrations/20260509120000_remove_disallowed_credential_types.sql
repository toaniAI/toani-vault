BEGIN;

DELETE FROM credentials
WHERE credential_type IN ('kyc_document', 'zk_kyc_credential');

DO $$
DECLARE
    schema_name text;
BEGIN
    FOR schema_name IN
        SELECT nspname
        FROM pg_namespace
        WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'public')
    LOOP
        IF EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = schema_name
              AND table_name = 'credentials'
        ) THEN
            EXECUTE format(
                'DELETE FROM %I.credentials WHERE credential_type IN (''kyc_document'', ''zk_kyc_credential'')',
                schema_name
            );
        END IF;
    END LOOP;
END $$;

COMMIT;
