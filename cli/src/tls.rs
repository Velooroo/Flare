//! TLS client used for every CLI → daemon connection.
//!
//! INFO: For local/private addresses we deliberately skip certificate
//!       verification (the daemon usually presents a self-signed cert). For
//!       public hosts we validate against the system root store.

use anyhow::Result;
use rustls::pki_types::{CertificateDer, ServerName};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

pub async fn connect(stream: TcpStream, host: &str) -> Result<TlsStream<TcpStream>> {
    let config = if is_local(host) {
        // skip verification for LAN
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerify))
            .with_no_client_auth()
    } else {
        // use system CA for internet hosts
        rustls::ClientConfig::builder()
            .with_root_certificates(ca_bundle())
            .with_no_client_auth()
    };

    let connector = TlsConnector::from(Arc::new(config));
    let domain = ServerName::try_from(host.to_string())?;

    Ok(connector.connect(domain, stream).await?)
}

// INFO: A host counts as "local" if it is `localhost` or a loopback/private IP.
//
// WARN: Hostnames such as `raspberrypi.local` are treated as public, so their
//       self-signed certificate is rejected. Use the device's IP address, or add
//       the cert to the trust store.
fn is_local(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }

    if let Ok(addr) = host.parse::<IpAddr>() {
        match addr {
            // NOTE: `is_private` covers 10/8, 172.16/12 and 192.168/16.
            IpAddr::V4(ipv4) => ipv4.is_loopback() || ipv4.is_private(),
            IpAddr::V6(ipv6) => ipv6.is_loopback(),
        }
    } else {
        false // it's domain name, not IP.
    }
}

fn ca_bundle() -> rustls::RootCertStore {
    let mut store = rustls::RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    store
}

// WARN: Accepts ANY server certificate. Safe-ish on a trusted LAN, but it means
//       there is no protection against a local man-in-the-middle.
#[derive(Debug)]
struct SkipVerify;

impl rustls::client::danger::ServerCertVerifier for SkipVerify {
    fn verify_server_cert(
        &self,
        _: &CertificateDer,
        _: &[CertificateDer],
        _: &ServerName,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
