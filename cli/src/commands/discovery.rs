//! `flare discover` and `flare sync` — find daemons on the LAN over UDP and pair
//! with them.
//!
//! INFO: Discovery broadcasts `FLARE_DISCOVER` to 255.255.255.255:7001; each
//!       daemon replies `FLARE_HERE:<tcp_port>`. Pairing then happens over TLS on
//!       that TCP port, where the CLI registers an argon2 hash of a fresh token
//!       and stores the plain token locally.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscoveredDevice {
    pub host: String,
    pub port: u16,
}

// INFO: Read-only: lists what was found and marks already-saved devices.
pub async fn discover() -> Result<()> {
    let found = scan_network().await?;

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

// INFO: Re-scans the network, then pairs with the devices at the given indices.
//       `range` is written like `0`, `0,2` or `0-3`.
//
// INFO: Pairing is protected by the daemon: a fresh daemon requires the one-time
//       code it printed at startup (`--code`), and an already-paired daemon
//       requires an existing token.
pub async fn sync(range: String, code: Option<String>) -> Result<()> {
    // re-run discovery to get fresh list
    println!("Re-discovering devices...\n");
    let found = scan_network().await?; // Search devices

    let indices = parse_range(&range)?;
    let mut config = common::load_config()?;
    let mut synced = 0;

    // INFO: Resolve the pairing code once (prompt if not given on the CLI).
    let pairing_code = match code {
        Some(c) => Some(c),
        None => {
            print!("Pairing code (first-time setup; leave blank if already paired): ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                Some(line.to_string())
            }
        }
    };

    for idx in indices {
        let device = found
            .get(idx as usize)
            .ok_or_else(|| anyhow::anyhow!("Device {} not found", idx))?;

        let token = common::generate_token();
        let token_hash = common::hash_token(&token)?;
        // NOTE: This logs the argon2 *hash*, not the token (the message is a bit
        //       misleading). Harmless, but consider `debug!` to keep it quiet.
        info!("Generate token: {:?}", token_hash);

        // INFO: If we already hold a token for this daemon, present it so an
        //       already-paired daemon accepts the new enrollment.
        let existing = common::find_device_by_addr(&device.host, device.port).and_then(|d| d.token);

        // send hash to daemon
        let registered = register_token(
            &device.host,
            device.port,
            &token_hash,
            existing.as_deref(),
            pairing_code.as_deref(),
        )
        .await?;

        if !registered {
            println!(
                "Failed to register token for {}. \
                 For a fresh device, pass its startup pairing code via --code. \
                 If it is already paired, sync from a machine that holds its token.",
                device.host
            );
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

// INFO: Sends the broadcast and collects replies for 2 seconds.
// WARN: Deduplicates by host only, so two daemons on one machine collapse into one.
pub async fn scan_network() -> Result<Vec<DiscoveredDevice>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;
    socket
        .send_to(b"FLARE_DISCOVER", "255.255.255.255:7001")
        .await?;

    let mut found = Vec::new();
    let mut buf = [0u8; 256];

    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok((len, addr)) = socket.recv_from(&mut buf).await {
                let response = String::from_utf8_lossy(&buf[..len]);
                let port = response
                    .strip_prefix("FLARE_HERE:")
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(7530);

                let device = DiscoveredDevice {
                    host: addr.ip().to_string(),
                    port,
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

    Ok(found)
}

// INFO: Parses `0`, `0,2` or `0-3` into a list of indices.
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

// INFO: Opens a short TLS connection just to register the token hash.
async fn register_token(
    host: &str,
    port: u16,
    token_hash: &str,
    existing_token: Option<&str>,
    pairing_code: Option<&str>,
) -> Result<bool> {
    let client = crate::commands::connect(host, port).await?;

    let reply = client
        .register_token(
            token_hash.to_string(),
            existing_token.map(|s| s.to_string()),
            pairing_code.map(|s| s.to_string()),
        )
        .await?;

    Ok(reply.success)
}
