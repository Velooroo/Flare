use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    pub user: Option<String>,
    pub password: Option<String>,
    pub forge: Option<String>,
}

fn auth_path() -> std::path::PathBuf {
    common::flare_dir().join("auth.toml")
}

pub fn login() -> Result<()> {
    println!("Flare Authentication");
    println!("---");

    print!("Username: ");
    io::stdout().flush()?;
    let mut user = String::new();
    io::stdin().read_line(&mut user)?;
    let user = user.trim().to_string();

    print!("Password/Token: ");
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    print!("Forge URL (optional, default: github): ");
    io::stdout().flush()?;
    let mut forge = String::new();
    io::stdin().read_line(&mut forge)?;
    let forge = forge.trim();

    let auth = AuthConfig {
        user: if user.is_empty() { None } else { Some(user) },
        password: if password.is_empty() {
            None
        } else {
            Some(password)
        },
        forge: if forge.is_empty() {
            None
        } else {
            Some(forge.to_string())
        },
    };

    let path = auth_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, toml::to_string_pretty(&auth)?)?;

    println!("\n✓ Credentials saved to {:?}", path);
    Ok(())
}

pub fn logout() -> Result<()> {
    let path = auth_path();

    if path.exists() {
        std::fs::remove_file(&path)?;
        println!("✓ Logged out");
    } else {
        println!("Not logged in");
    }

    Ok(())
}

pub fn status() -> Result<()> {
    let path = auth_path();

    if !path.exists() {
        println!("Not logged in");
        println!("Run: flare auth login");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let auth: AuthConfig = toml::from_str(&content)?;

    println!(
        "Logged in as: {}",
        auth.user.as_deref().unwrap_or("<no user>")
    );
    println!("Forge: {}", auth.forge.as_deref().unwrap_or("github"));
    println!(
        "Token: {}",
        if auth.password.is_some() {
            "set"
        } else {
            "not set"
        }
    );

    Ok(())
}

pub fn load() -> Result<AuthConfig> {
    let path = auth_path();

    if !path.exists() {
        return Ok(AuthConfig::default());
    }

    let content = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&content)?)
}
