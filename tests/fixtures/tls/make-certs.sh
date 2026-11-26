#!/bin/sh
# Test material for the fake HTTPS service (D-157): a certificate authority
# and a server certificate for localhost and 127.0.0.1 signed by it, valid
# for 100 years. The CA's own key is deleted at the end: nothing more can be
# signed with it. The server key protects nothing; it is test material.
# These files were copied from experiments/m21-https, made the same way.
set -eu
cd "$(dirname "$0")"
O=${OPENSSL:-openssl}
$O req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 36500 -subj "/CN=norn test CA" -keyout ca-key.pem -out ca.pem
$O req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -subj "/CN=server" -keyout server-key.pem -out server.csr
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' > server.ext
$O x509 -req -in server.csr -CA ca.pem -CAkey ca-key.pem -CAcreateserial -days 36500 -extfile server.ext -out server.pem
rm -f server.csr server.ext ca.srl ca-key.pem
