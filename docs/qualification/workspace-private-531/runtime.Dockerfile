FROM ubuntu
COPY --from=debs / /debs
RUN dpkg -i /debs/*.deb \
    && rm -rf /debs \
    && test -x /lib/ld-linux-aarch64.so.1 \
    && python3 -c "import ssl; import os; assert os.getuid() == 0" \
    && /usr/bin/stat --version >/dev/null \
    && /usr/bin/printenv --version >/dev/null

USER root
WORKDIR /workspace
