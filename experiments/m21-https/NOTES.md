# E5 — HTTPS from the runtime with `ureq`

**Question (v3 scope §4, E5).** For M21 the runtime itself sends `ext.call`
requests over HTTPS. norn already uses `ureq` 3 for HTTP. Does its default
TLS (encryption plus a check of the server's certificate) verify certificates
properly? Against which list of trusted authorities? And can it be told to
trust one test authority, for the local fake HTTPS service of the trial
(D-153), without a new dependency?

**What it is.** `src/main.rs` starts two tiny HTTPS servers on `127.0.0.1`
with `rustls` (the TLS library `ureq` already uses), and sends eight requests.
`make-certs.sh` makes a test certificate authority (CA: the party that signs
certificates) and two server certificates signed by it. Run:

```sh
OPENSSL=<path to openssl> ./make-certs.sh
nix develop -c cargo run          # from this folder
```

## What it showed (2026-11-26, NixOS, kernel 7.1.8)

```
ureq default root store: WebPki
1 default agent, https://pypi.org/simple/               HTTP 200 OK
2 default agent, https://self-signed.badssl.com/        error: io: invalid peer certificate: UnknownIssuer
3 default agent, local self-signed                      error: io: invalid peer certificate: UnknownIssuer
4 agent trusting ca.pem only, local                     HTTP 200 OK
5 agent trusting ca.pem only, via name localhost        HTTP 200 OK
6 agent trusting ca.pem only, https://pypi.org          error: io: invalid peer certificate: UnknownIssuer
7 trusting ca.pem, served for other.example             error: io: invalid peer certificate: certificate not valid for name "127.0.0.1"; certificate is only valid for DnsName("other.example")
8 default agent, http:// to the TLS port                error: protocol: http parse fail: invalid HTTP version
```

- `ureq`'s default trusted list is **`WebPki`**: Mozilla's list of
  authorities, compiled into the program (crate `webpki-roots`, already in
  `Cargo.lock`). It does not read the system's certificate store at all. So
  the answer to "does it verify against the system store on NixOS, Ubuntu and
  Debian" is: it does not use the system store, on any of them; it carries
  its own list, so the result cannot differ between distributions. Only
  NixOS was run here; the sameness elsewhere is reasoning from that fact.
- An unknown authority (2, 3) and a certificate for another name (7) are
  refused. A real host (1) works.
- `RootCerts::new_with_certs` makes an agent that trusts **only** the given
  authority (4, 5), and then a real host is refused (6). That is the shape a
  per-service "trust this CA" setting should have: it narrows, it does not add.
- **First attempt failed:** a single self-signed certificate made with
  `openssl req -x509` was refused even when given as the trusted root, with
  `CaUsedAsEndEntity`: `openssl` marks such a certificate as a CA, and the
  verifier (`webpki`) refuses a CA certificate used as a server's. A test
  setup needs a CA and a server certificate signed by it, which is what
  `make-certs.sh` now makes.

## Decisions it leads to

- **No new dependency for the client.** `ureq` with its default TLS is
  enough; the compiled-in Mozilla list is the trusted list for remote
  services.
- **A service may name its own CA file** (`ca_file` in the policy). The
  runtime then trusts that CA only, for that service. This is how the local
  fake HTTPS service is reached in tests and in the trial; nothing is ever
  "trust any certificate".
- **The fake HTTPS service in the tests uses `rustls` directly**, as a
  development dependency. It is already in the dependency tree through
  `ureq` (same version, same `ring` back end), so nothing new is downloaded or
  built for the program itself.
- The test CA, its server certificate and the server's key go into
  `tests/fixtures/tls/`, with the script that made them. They are test
  material: the key protects nothing, and the CA's own key is not kept.
