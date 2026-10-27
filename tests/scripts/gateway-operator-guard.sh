#!/bin/sh
# The operator gateway lifecycle tests start a real operator daemon. A fresh
# HOME isolates its state, but not the machine's privileged bind broker or
# 127.0.0.1:80/:443, so they run only while no gateway is listening there and
# never stop a running one themselves (papercut 06d1cd9e).
set -eu

refuse() {
    printf '%s\n' \
        "refusing to run the operator gateway lifecycle tests: $1" \
        "These tests start a real operator daemon and never stop a running gateway." \
        "Use the private gateway tests while the operator gateway is in use." >&2
    exit 1
}

command -v lsof >/dev/null 2>&1 || refuse "lsof is unavailable; listener isolation cannot be checked."

for port in 80 443; do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
        refuse "TCP port $port already has a listener."
    fi
done

if lsof -nP -iUDP:15353 >/dev/null 2>&1; then
    refuse "UDP port 15353 already has a listener."
fi
