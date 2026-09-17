//! Shared CLI rendering helpers.

/// Print a service backup table.
///
/// Every service type shares this so the columns cannot drift apart; adding a
/// field to `ServiceBackup` means updating exactly one place.
pub fn print_backup_table(data: &serde_json::Value) {
    let Some(backups) = data.as_array() else {
        println!("Unexpected response.");
        return;
    };
    if backups.is_empty() {
        println!("No backups.");
        return;
    }

    println!(
        "{:<36} {:<12} {:<20} {:<14} RESTORED",
        "ID", "SIZE", "CREATED", "ENCRYPTION"
    );
    println!("{}", "-".repeat(112));
    for backup in backups {
        println!(
            "{:<36} {:<12} {:<20} {:<14} {}",
            backup["id"].as_str().unwrap_or("-"),
            backup["size_bytes"].as_i64().unwrap_or_default(),
            backup["created_at"].as_str().unwrap_or("-"),
            backup["encryption"].as_str().unwrap_or("none"),
            backup["restored_at"].as_str().unwrap_or("never"),
        );
    }
}
