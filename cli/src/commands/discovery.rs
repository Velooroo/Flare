use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::UdpSocket;
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscoveredDevice {
    pub host: String,
    pub port: u16,
}

pub async fn discover() -> Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    info!("Searching for devices...\n");

    socket
        .send_to(b"FLARE_DISCOVER", "255.255.255.255:7001")
        .await?;

    let mut found = Vec::new();
    let mut buf = [0u8; 256];

    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok((_, addr)) = socket.recv_from(&mut buf).await {
                let device = DiscoveredDevice {
                    host: addr.ip().to_string(),
                    port: 7530,
                };

                // avoid duplicates
                if !found
                    .iter()
                    .any(|d: &DiscoveredDevice| d.host == device.host)
                {
                    found.push(device);
                }
            }
        }
    })
    .await;

    if found.is_empty() {
        println!("No devices found");
        return Ok(());
    }

    // load saved config
    let config = common::load_config()?;

    // print results
    for (i, device) in found.iter().enumerate() {
        let saved = config
            .devices
            .iter()
            .find(|d| d.host == device.host && d.port == device.port);

        if let Some(s) = saved {
            let name = s.name.as_deref().unwrap_or("unnamed");
            println!("[{}] {}:{} (saved: {})", i, device.host, device.port, name);
        } else {
            println!("[{}] {}:{} (new)", i, device.host, device.port);
        }
    }

    println!("\nTo add devices, run: flare sync <range>");
    println!("Example: flare sync 0-2");

    Ok(())
}

pub async fn sync(range: &str) -> Result<()> {
    // re-run discovery to get fresh list
    println!("Re-discovering devices...\n");

    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;
    socket
        .send_to(b"FLARE_DISCOVER", "255.255.255.255:7001")
        .await?;

    let mut found = Vec::new();
    let mut buf = [0u8; 256];

    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok((_, addr)) = socket.recv_from(&mut buf).await {
                let device = DiscoveredDevice {
                    host: addr.ip().to_string(),
                    port: 7530,
                };

                if !found
                    .iter()
                    .any(|d: &DiscoveredDevice| d.host == device.host)
                {
                    found.push(device);
                }
            }
        }
    })
    .await;

    if found.is_empty() {
        anyhow::bail!("No devices found. Run 'flare discover' first");
    }

    let indices = parse_range(range)?;
    let mut config = common::load_config()?;
    let mut synced = 0;

    for idx in indices {
        let device = found
            .get(idx as usize)
            .ok_or_else(|| anyhow::anyhow!("Device {} not found", idx))?;

        let token = common::generate_token();
        let token_hash = common::hash_token(&token)?;

        // send hash to daemon
        let registered = register_token(&device.host, device.port, &token_hash).await?;

        if !registered {
            println!("Failed to register token for {}", device.host);
            continue;
        }

        // get name
        print!("Name (optional): ");
        io::stdout().flush()?;
        let mut name = String::new();
        io::stdin().read_line(&mut name)?;
        let name = name.trim();

        // save device with plain token
        let new_device = common::Device {
            id: common::next_device_id(&config),
            name: if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            },
            host: device.host.clone(),
            port: device.port,
            token: Some(token), // plain token
        };

        config.devices.push(new_device);
        println!("SUCCESS: Saved\n");
        synced += 1;
    }

    common::save_config(&config)?;
    println!("Synced {} devices", synced);

    Ok(())
}

fn parse_range(range: &str) -> Result<Vec<u32>> {
    let mut result = Vec::new();

    for part in range.split(',') {
        if part.contains('-') {
            let bounds: Vec<&str> = part.split('-').collect();
            if bounds.len() != 2 {
                anyhow::bail!("Invalid range: {}", part);
            }
            let start: u32 = bounds[0].parse()?;
            let end: u32 = bounds[1].parse()?;
            result.extend(start..=end);
        } else {
            result.push(part.parse()?);
        }
    }

    Ok(result)
}

async fn register_token(host: &str, port: u16, token_hash: &str) -> Result<bool> {
    use common::{RegisterTokenRequest, RegisterTokenResponse};

    let tcp = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let mut socket = crate::tls::connect(tcp, host).await?;

    let req = RegisterTokenRequest {
        msg_type: "register_token".into(),
        token_hash: token_hash.to_string(),
    };

    common::send_json(&mut socket, &req).await?;
    let resp: RegisterTokenResponse = common::recv_json(&mut socket).await?;

    Ok(resp.success)
}
