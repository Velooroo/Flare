use anyhow::Result;
use tokio::net::UdpSocket;
use tracing::info;

pub async fn run(port: u16) -> Result<()> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Discovery listening on UDP {}", port);

    let mut buf = [0u8; 64];

    loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        let msg = String::from_utf8_lossy(&buf[..len]);

        if msg == "FLARE_DISCOVER" {
            info!("Discovery ping from {}", addr);
            socket.send_to(b"FLARE_HERE", addr).await?;
        }
    }
}
