#!/bin/sh
set -e

# 1 - files prefix
#     may include path to destination folder
#     or may be used to generate multiple bundles at the same location

prefix="${1}"
basedir=$(dirname ${0})

openssl req -nodes -x509 -days 3650 -sha256 -batch -subj "/CN=Test RSA root CA" \
            -newkey rsa:4096 -keyout ${prefix}ca.key -out ${prefix}ca.crt

openssl req -nodes -sha256 -batch -subj "/CN=Test RSA intermediate CA" \
            -newkey rsa:3072 -keyout ${prefix}inter.key -out ${prefix}inter.req

openssl req -nodes -sha256 -batch -subj "/CN=test-server.com" \
            -newkey rsa:2048 -keyout ${prefix}end.key -out ${prefix}end.req

openssl rsa -in ${prefix}end.key -out ${prefix}test-server.key

openssl x509 -req -sha256 -days 3650 -set_serial 123 -extensions v3_inter -extfile ${basedir}/openssl.cnf \
             -CA ${prefix}ca.crt -CAkey ${prefix}ca.key -in ${prefix}inter.req -out ${prefix}inter.crt

openssl x509 -req -sha256 -days 2000 -set_serial 456 -extensions v3_end -extfile ${basedir}/openssl.cnf \
             -CA ${prefix}inter.crt -CAkey ${prefix}inter.key -in ${prefix}end.req -out ${prefix}end.crt

cat ${prefix}end.crt ${prefix}inter.crt > ${prefix}test-server.pem
cat ${prefix}inter.crt ${prefix}ca.crt > ${prefix}ca.pem
rm ${prefix}*.req ${prefix}ca.key ${prefix}inter.key ${prefix}end.key

# tests/tls/ is a fixed path shared by every build.rs invocation (test fixtures
# reference it directly, so it can't be scoped to this run's OUT_DIR). Without
# coordination, concurrent invocations of this script -- e.g. an IDE's
# background `cargo check` racing a terminal `cargo test` -- can interleave
# their mkdir/cp here and leave tests/tls/ empty or holding a CA/server-cert
# pair from two different runs, which then fails TLS handshakes. Serialize the
# publish step with a portable mkdir-based lock; if it's still held after 15s
# (should never happen, the critical section is a couple of file copies),
# proceed anyway rather than deadlock.
lock_dir="tests/.tls.lock"
lock_acquired=0
attempt=0
while [ "${attempt}" -lt 150 ]; do
    if mkdir "${lock_dir}" 2>/dev/null; then
        lock_acquired=1
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.1
done
if [ "${lock_acquired}" -eq 1 ]; then
    trap 'rmdir "${lock_dir}" 2>/dev/null || true' EXIT INT TERM
fi

mkdir -p tests/tls
cp ${prefix}ca.* tests/tls/
