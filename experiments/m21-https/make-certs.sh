#!/bin/sh
# A test CA and two server certificates signed by it, valid for 100 years.
# webpki (ureq's verifier) refuses a certificate that is its own CA
# (CaUsedAsEndEntity), so a single self-signed certificate is not enough.
set -eu
O=${OPENSSL:-openssl}
$O req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 36500 -subj "/CN=norn test CA" -keyout ca-key.pem -out ca.pem
leaf() { # name SAN
  $O req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -subj "/CN=$1" -keyout $1-key.pem -out $1.csr
  printf 'subjectAltName=%s\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' "$2" > $1.ext
  $O x509 -req -in $1.csr -CA ca.pem -CAkey ca-key.pem -CAcreateserial -days 36500 -extfile $1.ext -out $1.pem
  rm -f $1.csr $1.ext
}
leaf server "DNS:localhost,IP:127.0.0.1"
leaf other "DNS:other.example"
rm -f ca.srl
