#!/bin/sh
set -eu

EMDASH_DATA_DIR="${EMDASH_DATA_DIR:-/data/emdash}"
EMDASH_DB_PATH="${EMDASH_DB_PATH:-${EMDASH_DATA_DIR}/data.db}"
EMDASH_UPLOADS_DIR="${EMDASH_UPLOADS_DIR:-${EMDASH_DATA_DIR}/uploads}"

export EMDASH_DATA_DIR
export EMDASH_DB_PATH
export EMDASH_UPLOADS_DIR

mkdir -p "${EMDASH_UPLOADS_DIR}"

if [ "${EMDASH_AUTO_BOOTSTRAP:-true}" = "true" ] && [ ! -f "${EMDASH_DB_PATH}" ]; then
  echo "Initializing EmDash data store at ${EMDASH_DB_PATH}"
  ./node_modules/.bin/emdash init
  ./node_modules/.bin/emdash seed
fi

exec node ./dist/server/entry.mjs
