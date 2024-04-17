use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                CREATE OR REPLACE FUNCTION base58_encode (data bytea)
                RETURNS text AS
                $body$
                DECLARE
                  -- Alphabet for Base58 encoding
                  alphabet text[] = ARRAY[
                    '1','2','3','4','5','6','7','8','9',
                    'A','B','C','D','E','F','G','H','J','K','L','M','N','P','Q','R','S','T','U','V','W','X','Y','Z',
                    'a','b','c','d','e','f','g','h','i','j','k','m','n','o','p','q','r','s','t','u','v','w','x','y','z'
                  ];
                  base58_count integer := 58;
                  encoded_text text := '';
                  hex_string text;
                  numeric_value numeric;
                  remainder numeric;
                BEGIN
                  -- Convert data to hex string and then to numeric
                  hex_string := encode(data, 'hex');
                  numeric_value := hex_to_numeric(hex_string);

                  -- Perform Base58 encoding
                  WHILE (numeric_value >= base58_count) LOOP
                    remainder := numeric_value % base58_count;
                    numeric_value := (numeric_value - remainder) / base58_count;
                    encoded_text := alphabet[(remainder + 1)] || encoded_text;
                  END LOOP;

                  -- Add the remaining numeric value to the encoded text
                  RETURN alphabet[(numeric_value + 1)] || encoded_text;
                END;
                $body$
                LANGUAGE plpgsql;
                "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                CREATE OR REPLACE FUNCTION hex_to_numeric(hex_string TEXT) RETURNS NUMERIC AS $$
                DECLARE
                    -- Process 8 characters at a time
                    chunk_size INTEGER := 8;
                    total_length INTEGER := LENGTH(hex_string);
                    result NUMERIC := 0;
                    chunk_start INTEGER := 1;
                BEGIN
                    WHILE chunk_start <= total_length LOOP
                        -- Get the next chunk
                        DECLARE
                            chunk TEXT := SUBSTRING(hex_string FROM chunk_start FOR chunk_size);
                            chunk_value NUMERIC := ( ('x' || chunk)::bit(32)::bigint::dec );
                        BEGIN
                            -- Scale the chunk value by 2^32 and add it to the result
                            result := result * POW(2::numeric, 32) + chunk_value;
                            chunk_start := chunk_start + chunk_size;
                        EXCEPTION
                            WHEN invalid_text_representation THEN
                                RAISE EXCEPTION 'Invalid hex string';
                        END;
                    END LOOP;

                    RETURN result;
                END;
                $$ LANGUAGE plpgsql;
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP FUNCTION IF EXISTS encode_base58(bytea_value bytea);
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }
}
