//! E5: does `ureq` (norn's HTTP client, with its default TLS) check
//! certificates as it should, and can it be told to trust one test
//! certificate for a local fake HTTPS service? See NOTES.md.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// A one-thread HTTPS server on 127.0.0.1 that answers "ok" to anything.
fn serve(cert: &str, key: &str) -> u16 {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert).unwrap().map(|c| c.unwrap()).collect();
    let key = PrivateKeyDer::from_pem_file(key).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let conn = rustls::ServerConnection::new(config.clone()).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, tcp);
            let mut reader = BufReader::new(&mut tls);
            let mut line = String::new();
            // Read the request head; a failed handshake ends here.
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line == "\r\n" => break,
                    Ok(_) => {}
                }
            }
            let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            let _ = tls.flush();
        }
    });
    port
}

fn get(name: &str, agent: &ureq::Agent, url: &str) {
    match agent.get(url).call() {
        Ok(r) => println!("{name:<55} HTTP {}", r.status()),
        Err(e) => println!("{name:<55} error: {e}"),
    }
}

fn agent_with(roots: Option<&str>) -> ureq::Agent {
    let mut tls = ureq::tls::TlsConfig::builder();
    if let Some(pem) = roots {
        let pem = std::fs::read(pem).unwrap();
        let cert = ureq::tls::Certificate::from_pem(&pem).unwrap();
        tls = tls.root_certs(ureq::tls::RootCerts::new_with_certs(&[cert]));
    }
    ureq::Agent::config_builder().tls_config(tls.build()).proxy(None).build().into()
}

fn main() {
    let good = serve("server.pem", "server-key.pem");
    let wrong_name = serve("other.pem", "other-key.pem");
    let default = agent_with(None);
    let trusting = agent_with(Some("ca.pem"));
    let trusting_other = agent_with(Some("ca.pem"));
    println!("ureq default root store: {:?}", ureq::tls::TlsConfig::default().root_certs());
    get("1 default agent, https://pypi.org/simple/", &default, "https://pypi.org/simple/six/");
    get("2 default agent, https://self-signed.badssl.com/", &default, "https://self-signed.badssl.com/");
    get("3 default agent, local self-signed", &default, &format!("https://127.0.0.1:{good}/"));
    get("4 agent trusting ca.pem only, local", &trusting, &format!("https://127.0.0.1:{good}/"));
    get("5 agent trusting ca.pem only, via name localhost", &trusting, &format!("https://localhost:{good}/"));
    get("6 agent trusting ca.pem only, https://pypi.org", &trusting, "https://pypi.org/simple/six/");
    get("7 trusting ca.pem, served for other.example", &trusting_other, &format!("https://127.0.0.1:{wrong_name}/"));
    get("8 default agent, http:// to the TLS port", &default, &format!("http://127.0.0.1:{good}/"));
}
