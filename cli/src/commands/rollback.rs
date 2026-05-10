pub fn rollback(app: &str) -> Result<()> {
    // Rollback don't have full functionality
    // TODO: it will be necessary to transfer to the deployment part
    let dir = common::app_dir(app);
    let versions = dir.join("versions");

    let mut entries: Vec<_> = std::fs::read_dir(&versions)?
        .filter_map(|e| e.ok())
        .collect();

    entries.sort_by_key(|e| e.path());

    let latest = entries
        .last()
        .ok_or_else(|| anyhow::anyhow!("No backups found"))?;

    let current = dir.join("current");

    if current.exists() {
        std::fs::remove_file(&current)?;
    }

    std::os::unix::fs::symlink(latest.path(), &current)?;

    info!("Rolled back: {}", app);
    Ok(())
}