# E1 — Package tools through an HTTPS `CONNECT` proxy

**Question (v3 scope §4, E1).** Do `pip`, `npm`, `cargo` and `git` work through
an HTTP proxy given by `HTTPS_PROXY`, with only their registry hosts allowed?
Which hosts does each one really need?

**What it is.** `proxy.py`: about 70 lines of Python. It listens on
`127.0.0.1:PORT`, takes `CONNECT host:port` (a tunnel for HTTPS: the proxy sees
the host name and the bytes each way, never the content) and plain `GET
http://...`, refuses hosts not on the list with `403`, and writes one line per
connection: `OPEN` when the tunnel starts, `ALLOW ... up=N down=M` when it
ends, `DENY` when refused. With no hosts given it allows all (discovery mode).

```sh
python3 proxy.py 3199 log.txt pypi.org files.pythonhosted.org
HTTPS_PROXY=http://127.0.0.1:3199 HTTP_PROXY=http://127.0.0.1:3199 <tool> ...
```

Run 2026-11-26 on the development machine (NixOS, kernel 7.1.8), with a fresh
cache folder for each tool so that everything had to be downloaded.

## What it showed

| Tool | Command | Hosts it connected to | Result |
|---|---|---|---|
| pip 26.2.1 | `pip install six` / `attrs` in a fresh venv | `pypi.org:443` (index), `files.pythonhosted.org:443` (the files) | installed |
| npm | `npm install left-pad` / `is-even` with a fresh `--cache` | `registry.npmjs.org:443` only (up to 5 tunnels at once) | installed |
| cargo 1.95 | `cargo fetch` of `itoa = "1"` with a fresh `CARGO_HOME` | `index.crates.io:443` (sparse index), `static.crates.io:443` (the crate files) | downloaded |
| git | `git clone --depth 1 https://github.com/dtolnay/itoa` | `github.com:443` | cloned |

All four read `HTTPS_PROXY` / `https_proxy` from the environment with no other
setting. (`npm config get https-proxy` prints `null` and npm still uses the
variable.) None of them tried a host outside this table, and none did its own
DNS lookups that mattered: the proxy is given a host name and resolves it.

With the list set to those six hosts:

- `pip install attrs` and `npm install is-odd` worked.
- `curl https://example.com` got `CONNECT tunnel failed, response 403` and
  `curl http://example.com` got `403`; both were logged as `DENY`.

With the list narrowed to `pypi.org` alone, `pip install iniconfig` failed:
the index answered, then `DENY CONNECT files.pythonhosted.org:443`, and pip
said "This is likely a proxy configuration issue". So a too-short list fails
closed, and the log names the missing host.

## What this means for M19

- A `CONNECT` proxy with a host allow-list is enough for the four tools; no
  tool needs anything beyond `HTTPS_PROXY`/`HTTP_PROXY` (both spellings, lower
  and upper case, are set).
- The registries to document in the README and the presets' comments:
  Python `pypi.org`, `files.pythonhosted.org`; Node `registry.npmjs.org`;
  Rust `index.crates.io`, `static.crates.io`; git over HTTPS the forge's host.
- A connection's byte counts are known only when it closes, and a tool may
  keep connections open until it exits (npm keeps up to five). The log line
  for a connection is therefore written when it ends; one that is still open
  when the command ends is closed and logged by the runtime then.
- A refusal should be logged at the moment it happens, with the host, so the
  user can see which host to add.

## Not tried

`git` over SSH (it does not use an HTTP proxy at all; with the empty network
namespace it simply cannot connect, which is what we want). Private
registries and mirrors. `pip` with `--index-url` pointing elsewhere: it would
need that host on the list, which is the intended behaviour.
