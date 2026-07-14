#!/bin/sh

# Check if required environment variables are set
if [ -z "$DB_USER" ] || [ -z "$DB_NAME" ] || [ -z "$DB_PASSWORD" ] || [ -z "$DB_HOST" ] || [ -z "$DB_PORT" ]; then
  echo "Error: Required environment variables are not set."
  echo "Ensure DB_USER, DB_NAME, DB_PASSWORD, DB_HOST, and DB_PORT are set."
  exit 1
fi

# Construct the DB_URL with the new user
export DB_URL=postgresql://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME

# Check if SQUID_ENABLED is true
if [ "$SQUID_ENABLED" = "true" ]; then
    echo "Starting indexer & GraphQL API..."
    exec ./indexer.sh
else
    echo "Starting squid GraphQL API..."
    exec sqd serve:prod
fi