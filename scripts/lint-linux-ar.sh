#!/bin/sh
# lint:linux archiver wrapper: companion to lint-linux-cc.sh for static libs.
#
# Used only as AR_aarch64_unknown_linux_gnu inside scripts/lint-linux.sh.
set -eu
exec zig ar "$@"
