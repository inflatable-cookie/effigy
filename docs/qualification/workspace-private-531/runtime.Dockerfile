FROM ubuntu
COPY --from=debs / /debs
RUN dpkg -i \
    /debs/libpython3-stdlib_3.10.6-1~22.04.1_arm64.deb \
    /debs/python3.10_3.10.12-1~22.04.18_arm64.deb \
    /debs/python3.10-minimal_3.10.12-1~22.04.18_arm64.deb \
    /debs/libpython3.10-minimal_3.10.12-1~22.04.18_arm64.deb \
    /debs/libpython3.10-stdlib_3.10.12-1~22.04.18_arm64.deb \
    /debs/ca-certificates_20260816ubuntu0.22.04.2_all.deb \
    /debs/openssl_3.0.2-0ubuntu1.30_arm64.deb \
    /debs/libssl3_3.0.2-0ubuntu1.30_arm64.deb \
    /debs/libexpat1_2.4.7-1ubuntu0.9_arm64.deb \
    /debs/libmpdec3_2.5.1-2build2_arm64.deb \
    /debs/libncursesw6_6.3-2ubuntu0.3_arm64.deb \
    /debs/libtinfo6_6.3-2ubuntu0.3_arm64.deb \
    /debs/libnsl2_1.3.0-2build2_arm64.deb \
    /debs/libreadline8_8.1.2-1_arm64.deb \
    /debs/readline-common_8.1.2-1_all.deb \
    /debs/libsqlite3-0_3.37.2-2ubuntu0.8_arm64.deb \
    /debs/libtirpc3_1.3.2-2ubuntu0.1_arm64.deb \
    /debs/libtirpc-common_1.3.2-2ubuntu0.1_all.deb \
    /debs/libdb5.3_5.3.28+dfsg1-0.8ubuntu3_arm64.deb \
    /debs/media-types_7.0.0_all.deb \
    && dpkg -i /debs/python3-minimal_3.10.6-1~22.04.1_arm64.deb \
    && dpkg -i /debs/python3_3.10.6-1~22.04.1_arm64.deb \
    && rm -rf /debs \
    && test "$(dpkg-query -W -f='${db:Status-Status}\n' libpython3-stdlib python3.10 python3.10-minimal libpython3.10-minimal libpython3.10-stdlib ca-certificates openssl libssl3 libexpat1 libmpdec3 libncursesw6 libtinfo6 libnsl2 libreadline8 readline-common libsqlite3-0 libtirpc3 libtirpc-common libdb5.3 media-types python3-minimal python3 | grep -c '^installed$')" = 22 \
    && test -x /lib/ld-linux-aarch64.so.1 \
    && python3 -c "import ssl; import os; assert os.getuid() == 0" \
    && /usr/bin/stat --version >/dev/null \
    && /usr/bin/printenv --version >/dev/null

USER root
WORKDIR /workspace
