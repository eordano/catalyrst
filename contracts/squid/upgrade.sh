# Load environment variables from .env file if it exists
if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi


SQUID_TIMESTAMP=$1
SQUID_SCHEMA="marketplace_squid_${SQUID_TIMESTAMP}"
SQUID_DB_USER="marketplace_squid_user_${SQUID_TIMESTAMP}"

export PGPASSWORD=$DB_PASSWORD

psql -v ON_ERROR_STOP=1 --username "$DB_USER" --dbname "$DB_NAME" --host "$DB_HOST" --port "$DB_PORT" <<-EOSQL
  DO \$\$
  DECLARE
      old_schema_name TEXT;
  BEGIN
    -- Fetch the old schema name from the table
    SELECT schema INTO old_schema_name FROM squids WHERE name = 'marketplace';
    
    -- Rename the old schema
    EXECUTE format('ALTER SCHEMA squid_marketplace RENAME TO %I', old_schema_name);
    
    -- Rename the new schema to the desired name
    EXECUTE format('ALTER SCHEMA %I RENAME TO squid_marketplace', '$SQUID_SCHEMA');
    
    -- Update the search path for the user
    EXECUTE format('ALTER USER %I SET search_path TO squid_marketplace', '$SQUID_DB_USER');
    
    UPDATE squids SET schema = '$SQUID_SCHEMA' WHERE name = 'marketplace';
    
  -- Commit the transaction
  COMMIT;
  END \$\$;
EOSQL
  


