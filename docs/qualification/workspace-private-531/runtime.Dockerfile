FROM ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11

RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 coreutils libc6 ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && test -x /lib/ld-linux-aarch64.so.1 \
    && python3 -c "import ssl; import os; assert os.getuid() == 0" \
    && /usr/bin/stat --version >/dev/null \
    && /usr/bin/printenv --version >/dev/null

USER root
WORKDIR /workspace
