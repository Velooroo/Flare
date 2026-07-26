use anyhow::Result;
use tokio::net::UdpSocket;
use tracing::info;

pub async fn run(discovery_port: u16, daemon_port: u16) -> Result<()> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", discovery_port)).await?;
    info!("Discovery listening on UDP {} (daemon TCP {})", discovery_port, daemon_port);

    let mut buf = [0u8; 64];

    loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        let msg = String::from_utf8_lossy(&buf[..len]);

        if msg == "FLARE_DISCOVER" {
            info!("Discovery ping from {}", addr);
            let response = format!("FLARE_HERE:{}", daemon_port);
            socket.send_to(response.as_bytes(), addr).await?;
        }
    }
}
