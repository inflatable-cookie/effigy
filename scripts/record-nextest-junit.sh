#!/bin/sh
# Preserve the most recent default-profile report under its owning QA selector.
set -eu
name=${1:?selector report name required}
source=target/nextest/default/qa-junit.xml
destination=target/nextest/qa
if [ ! -f "$source" ]; then
  echo "record-nextest-junit: expected report at $source" >&2
  exit 1
fi
mkdir -p "$destination"
cp "$source" "$destination/$name.xml"
