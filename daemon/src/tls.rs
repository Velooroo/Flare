//! TLS setup for the daemon control port.
//!
//! INFO: If $FLARE_TLS_CERT and $FLARE_TLS_KEY are set, they are loaded; otherwise
//!       a self-signed localhost certificate is generated.
//! INFO: The acceptor is built once at startup and reused for every connection.

use anyhow::Result;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tracing::info;

/// Builds the TLS acceptor once. Call this during startup, not per connection.
pub fn build_acceptor() -> Result<TlsAcceptor> {
    let (cert, key) = load_or_generate()?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub async fn accept(acceptor: &TlsAcceptor, stream: TcpStream) -> Result<TlsStream<TcpStream>> {
    Ok(acceptor.accept(stream).await?)
}

// try env vars first, fallback to self-signed
fn load_or_generate() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    if let (Ok(cert), Ok(key)) = (
        std::env::var("FLARE_TLS_CERT"),
        std::env::var("FLARE_TLS_KEY"),
    ) {
        match load_files(&cert, &key) {
            Ok(result) => return Ok(result),
            // WARN: A bad path is only warned about; we fall back to self-signed.
            Err(e) => info!("Failed to load TLS_CERT/KEY ({}); using self-signed", e),
        }
    }
    info!("No TLS_CERT configured; generating a self-signed certificate");
    generate_cert()
}

fn load_files(
    cert_path: &str,
    key_path: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    use std::io::BufReader;

    let certs = rustls_pemfile::certs(&mut BufReader::new(std::fs::File::open(cert_path)?))
        .collect::<Result<Vec<_>, _>>()?;

    let key = rustls_pemfile::private_key(&mut BufReader::new(std::fs::File::open(key_path)?))?
        .ok_or_else(|| anyhow::anyhow!("No key in file"))?;

    Ok((certs, key))
}

// INFO: Self-signed cert for the name `localhost`.
// WARN: Not persisted to disk, so it changes on every restart. Because the only
//       SAN is `localhost`, a verifying TLS client cannot match the device's
//       IP/hostname — which is why the CLI skips verification on the LAN.
fn generate_cert() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;

    let cert_der = CertificateDer::from(cert.cert.der().to_vec());

    let key_bytes = cert.signing_key.serialized_der().to_vec();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));

    Ok((vec![cert_der], key_der))
}
