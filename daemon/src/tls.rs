use anyhow::Result;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, server::TlsStream};

pub async fn accept(stream: TcpStream) -> Result<TlsStream<TcpStream>> {
    let (cert, key) = load_or_generate()?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert, key)?;

    let acceptor = TlsAcceptor::from(Arc::new(config));
    Ok(acceptor.accept(stream).await?)
}

// try env vars first, fallback to self-signed
fn load_or_generate() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    if let (Ok(cert), Ok(key)) = (
        std::env::var("FLARE_TLS_CERT"),
        std::env::var("FLARE_TLS_KEY"),
    ) {
        if let Ok(result) = load_files(&cert, &key) {
            return Ok(result);
        }
    }

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

// generates localhost cert valid for 30 days
fn generate_cert() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;

    let cert_der = CertificateDer::from(cert.cert.der().to_vec());

    let key_bytes = cert.signing_key.serialized_der().to_vec();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));

    Ok((vec![cert_der], key_der))
}
