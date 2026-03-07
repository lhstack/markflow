#!/bin/sh
set -eu

mkdir -p /app/data /app/logs /app/uploads

chown -R markflow:markflow /app/data /app/logs /app/uploads || true

exec su-exec markflow /app/markflow "$@"
