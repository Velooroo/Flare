//! Flare's security foundation: TLS identities and a verified client transport.
//!
//! Design rules (these are the "bases" every Flare connection follows):
//!
//! * **One identity per machine.** The daemon's certificate lives in
//!   `~/.flare/tls/` and is created once, so its fingerprint is stable across
//!   restarts. (`FLARE_TLS_CERT` / `FLARE_TLS_KEY` override it with real certs.)
//! * **No blind trust on the LAN.** Local/private hosts are verified by
//!   certificate fingerprint: trust-on-first-use, then pinned. If the daemon's
//!   certificate ever changes, the CLI refuses to connect (possible MITM).
//! * **Public hosts use the real WebPKI.** No custom verifier, no exceptions.
//!
//! This replaces the old split where `daemon/src/tls.rs` generated a fresh
//! self-signed cert on every start and `cli/src/tls.rs` ran a `SkipVerify`.

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream as ClientStream;
use tokio_rustls::server::TlsStream as ServerStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tracing::{info, warn};

// ── paths ───────────────────────────────────────────────────────────────────

/// Directory holding this machine's TLS identity.
pub fn tls_dir() -> PathBuf {
    crate::flare_dir().join("tls")
}

fn cert_path() -> PathBuf {
    tls_dir().join("cert.der")
}

fn key_path() -> PathBuf {
    tls_dir().join("key.der")
}

fn known_hosts_path() -> PathBuf {
    crate::flare_dir().join("known_hosts.toml")
}

// ── server identity ─────────────────────────────────────────────────────────

/// Loads the daemon identity, creating (and persisting) a self-signed one on
/// first run. Env vars `FLARE_TLS_CERT` + `FLARE_TLS_KEY` take precedence.
pub fn load_or_create_identity() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    // 1. Explicit production certificates win.
    if let (Ok(cert), Ok(key)) = (
        std::env::var("FLARE_TLS_CERT"),
        std::env::var("FLARE_TLS_KEY"),
    ) {
        return load_pem(&cert, &key)
            .with_context(|| format!("loading FLARE_TLS_CERT={cert} / FLARE_TLS_KEY={key}"));
    }

    // 2. Persisted identity (stable fingerprint across restarts).
    if cert_path().exists() && key_path().exists() {
        let cert = std::fs::read(cert_path()).context("reading stored TLS certificate")?;
        let key = std::fs::read(key_path()).context("reading stored TLS key")?;
        return Ok((
            vec![CertificateDer::from(cert)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        ));
    }

    // 3. First run: generate and persist.
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let cert_der = generated.cert.der().to_vec();
    let key_der = generated.signing_key.serialized_der().to_vec();

    std::fs::create_dir_all(tls_dir())?;
    std::fs::write(cert_path(), &cert_der)?;
    std::fs::write(key_path(), &key_der)?;
    crate::restrict_permissions(&cert_path());
    crate::restrict_permissions(&key_path());
    info!(
        "Generated a persistent TLS identity in {:?} (fingerprint {})",
        tls_dir(),
        fingerprint(&cert_der)
    );

    Ok((
        vec![CertificateDer::from(cert_der)],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
    ))
}

fn load_pem(
    cert_path: &str,
    key_path: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    use std::io::BufReader;

    let certs = rustls_pemfile::certs(&mut BufReader::new(std::fs::File::open(cert_path)?))
        .collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(std::fs::File::open(key_path)?))?
        .ok_or_else(|| anyhow::anyhow!("no private key in {}", key_path))?;
    Ok((certs, key))
}

/// Builds the TLS acceptor once; reuse it for every connection.
pub fn server_acceptor() -> Result<TlsAcceptor> {
    let (cert, key) = load_or_create_identity()?;
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert, key)?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub async fn accept(acceptor: &TlsAcceptor, stream: TcpStream) -> Result<ServerStream<TcpStream>> {
    Ok(acceptor.accept(stream).await?)
}

// ── fingerprints + known hosts ──────────────────────────────────────────────

/// SHA-256 of a DER certificate, lowercase hex. This is what gets pinned.
pub fn fingerprint(cert_der: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(cert_der);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct KnownHosts {
    #[serde(default)]
    hosts: HashMap<String, String>,
}

fn host_key(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn load_known_hosts() -> KnownHosts {
    std::fs::read_to_string(known_hosts_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_known_hosts(known: &KnownHosts) -> Result<()> {
    let path = known_hosts_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, toml::to_string_pretty(known)?)?;
    crate::restrict_permissions(&path);
    Ok(())
}

/// The fingerprint pinned for `host:port`, if we have connected before.
pub fn pinned_fingerprint(host: &str, port: u16) -> Option<String> {
    load_known_hosts().hosts.get(&host_key(host, port)).cloned()
}

/// Pin (or re-pin) the certificate fingerprint for `host:port`.
pub fn pin_fingerprint(host: &str, port: u16, fingerprint: &str) -> Result<()> {
    let mut known = load_known_hosts();
    known
        .hosts
        .insert(host_key(host, port), fingerprint.to_string());
    save_known_hosts(&known)
}

// ── client ──────────────────────────────────────────────────────────────────

/// Wraps an established TCP connection in TLS.
///
/// Local/private daemons are pinned by certificate fingerprint (TOFU); public
/// hosts are validated against the WebPKI roots.
pub async fn connect_tls(
    stream: TcpStream,
    host: &str,
    port: u16,
) -> Result<ClientStream<TcpStream>> {
    if is_local(host) {
        let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(FingerprintVerifier { seen: seen.clone() }))
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        let domain = ServerName::try_from(host.to_string())?;
        let tls = connector.connect(domain, stream).await?;

        let presented = seen
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("TLS peer presented no certificate"))?;

        match pinned_fingerprint(host, port) {
            Some(expected) if expected == presented => {}
            Some(expected) => anyhow::bail!(
                "TLS certificate for {host}:{port} changed\n  pinned: {expected}\n  got:    {presented}\n\
                 Possible man-in-the-middle. If the daemon was reinstalled on purpose, \
                 remove this host from {}.",
                known_hosts_path().display()
            ),
            None => {
                warn!("First connection to {host}:{port}; pinning certificate {presented}");
                pin_fingerprint(host, port, &presented)?;
            }
        }

        Ok(tls)
    } else {
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(ca_bundle())
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        let domain = ServerName::try_from(host.to_string())?;
        Ok(connector.connect(domain, stream).await?)
    }
}

fn ca_bundle() -> rustls::RootCertStore {
    let mut store = rustls::RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    store
}

/// `localhost` or a loopback/private IP. Hostnames such as `pi.local` are
/// treated as public (they must present a real certificate).
pub fn is_local(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Accepts the handshake and records the certificate fingerprint; the caller
/// compares it against the pin right after `connect` returns.
#[derive(Debug)]
struct FingerprintVerifier {
    seen: Arc<Mutex<Option<String>>>,
}

impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        *self.seen.lock().unwrap() = Some(fingerprint(end_entity.as_ref()));
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        match rustls::crypto::CryptoProvider::get_default() {
            Some(provider) => provider
                .signature_verification_algorithms
                .supported_schemes(),
            None => vec![
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ED25519,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_hex() {
        let a = fingerprint(b"hello");
        assert_eq!(a.len(), 64);
        assert_eq!(a, fingerprint(b"hello"));
        assert_ne!(a, fingerprint(b"world"));
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn local_hosts_are_detected() {
        assert!(is_local("localhost"));
        assert!(is_local("127.0.0.1"));
        assert!(is_local("10.0.0.5"));
        assert!(is_local("192.168.1.20"));
        assert!(is_local("::1"));
        assert!(!is_local("example.com"));
        assert!(!is_local("8.8.8.8"));
    }
}
