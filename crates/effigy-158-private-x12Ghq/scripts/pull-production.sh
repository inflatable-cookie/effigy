#!/bin/sh
set -eu
printf "%s" "$EFFIGY_CONTAINER_NAME" > "$PWD/pull-production.txt"
