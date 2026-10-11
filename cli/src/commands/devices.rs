//! `flare devices` — list and remove locally saved devices.

use anyhow::Result;
use common::load_config;

// INFO: `flare devices` with no sub-command lists the saved devices.
pub fn list() -> Result<()> {
    let config = load_config()?;

    if config.devices.is_empty() {
        println!("No devices configured");
        println!("Run: flare discover && flare sync <range>");
        return Ok(());
    }

    for d in &config.devices {
        let name = d.name.as_deref().unwrap_or("unnamed");
        println!("[{}] {:16} {}:{}", d.id, name, d.host, d.port);
    }

    Ok(())
}

// INFO: Accepts either a numeric id or a saved name.
//
// WARN: This only forgets the device locally; the token hash stays on the daemon
//       forever. There is no revoke endpoint yet.
pub fn remove(id: &str) -> Result<()> {
    let mut config = common::load_config()?;

    let idx = if let Ok(num) = id.parse::<u32>() {
        config.devices.iter().position(|d| d.id == num)
    } else {
        config
            .devices
            .iter()
            .position(|d| d.name.as_deref() == Some(id))
    };

    match idx {
        Some(i) => {
            let removed = config.devices.remove(i);
            common::save_config(&config)?;
            println!("Removed: {}", removed.host);
        }
        None => {
            println!("Device not found: {}", id);
        }
    }

    Ok(())
}
