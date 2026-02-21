#!/usr/bin/env bash
set -eo pipefail

DB_USER=${POSTGRES_USER:=qarax}
DB_PASSWORD="${POSTGRES_PASSWORD:=qarax}"
DB_NAME=${POSTGRES_DB:=qarax}
DB_PORT=${POSTGRES_PORT:=5432}

if [[ -z "${SKIP_DOCKER}" ]]; then
	CONTAINER_ID=$(docker run \
		-e POSTGRES_USER="${DB_USER}" \
		-e POSTGRES_PASSWORD="${DB_PASSWORD}" \
		-e POSTGRES_DB="${DB_NAME}" \
		-p "${DB_PORT}":5432 \
		-d docker.io/library/postgres:15)
	echo >&2 "Started Postgres container ${CONTAINER_ID}"
	until docker exec "${CONTAINER_ID}" pg_isready -U "${DB_USER}"; do
		echo >&2 "Postgres is still unavailable - sleeping"
		sleep 2
	done
else
	if command -v psql >/dev/null 2>&1; then
		export PGPASSWORD="${DB_PASSWORD}"
		until psql -h "localhost" -U "${DB_USER}" -p "${DB_PORT}" -d "postgres" -c '\q'; do
			echo >&2 "Postgres is still unavailable - sleeping"
			sleep 2
		done
	else
		until nc -z localhost "${DB_PORT}" 2>/dev/null; do
			echo >&2 "Postgres is still unavailable - sleeping"
			sleep 2
		done
	fi
fi

echo >&2 "Postgres is up and running on port ${DB_PORT} - running migrations now!"

export DATABASE_URL=postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_PORT}/${DB_NAME}
sqlx database create
echo "Created DB ${DATABASE_URL}"

sqlx mig run
echo "ran migrations"
echo >&2 "Postgres has been migrated, ready to go!"
