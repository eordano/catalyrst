#!/bin/sh

if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

SQUID_TIMESTAMP=$1
SQUID_SCHEMA="marketplace_squid_$SQUID_TIMESTAMP"
SQUID_DB_USER="marketplace_squid_user_$SQUID_TIMESTAMP"

if [ -z "$DB_USER" ] || [ -z "$DB_NAME" ] || [ -z "$DB_PASSWORD" ] || [ -z "$DB_HOST" ] || [ -z "$DB_PORT" ]; then
  echo "Error: Required environment variables are not set."
  echo "Ensure DB_USER, DB_NAME, DB_PASSWORD, DB_HOST, and DB_PORT are set."
  exit 1
fi

export DB_URL=postgresql://$SQUID_DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME
export DB_SCHEMA=$SQUID_SCHEMA

echo "Exported DB_URL: $DB_URL"
echo "Exported DB_SCHEMA: $DB_SCHEMA"

export CURRENT_SQUID_DB_USER=$SQUID_DB_USER
echo "Exported CURRENT_SQUID_DB_USER: $SQUID_DB_USER"

LOG_FILE="sqd_run_log_${SQUID_TIMESTAMP}.txt"
echo "Restarting squid services..."
nohup cpulimit -l 90 -- sqd run:marketplace > "$LOG_FILE" 2>&1 &

echo "Logs are being written to $LOG_FILE"
