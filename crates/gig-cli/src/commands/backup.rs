use gig_core::config::Paths;
use gig_core::Result;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(_conn: &Connection) -> Result<()> {
    let paths = Paths::from_env()?;
    paths.ensure_dirs()?;

    let db_path = &paths.db_file;
    if !db_path.exists() {
        return Err(gig_core::Error::Invalid(
            "database file does not exist".into(),
        ));
    }

    // Generate timestamp string YYYY-MM-DD-HHMMSS.
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ts_str = format_timestamp(ts);
    let backup_name = format!("gig-{ts_str}.db");
    let backup_path = paths.backups_dir.join(&backup_name);

    std::fs::copy(db_path, &backup_path)
        .map_err(|e| gig_core::Error::PathUnavailable(backup_path.clone(), e))?;

    let db_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    // Prune: keep only the 30 most recent backups.
    let kept = prune_backups(&paths)?;

    println!("backup: {}", backup_path.display());
    println!("size  : {} bytes", db_size);
    println!("total : {} backup(s)", kept);

    Ok(())
}

fn format_timestamp(secs: u64) -> String {
    // Simple UTC calendar calculation.
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days as i64);
    format!(
        "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
        year, month, day, h, m, s
    )
}

fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Delete oldest backups, keeping at most 30. Returns total count after pruning.
fn prune_backups(paths: &Paths) -> Result<usize> {
    let dir = &paths.backups_dir;
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| gig_core::Error::PathUnavailable(dir.clone(), e))?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("gig-"))
        .collect();

    // Sort by name (which is timestamp-based, so lexicographic = chronological).
    entries.sort_by_key(|e| e.file_name());

    let keep = 30;
    if entries.len() > keep {
        for entry in &entries[..entries.len() - keep] {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    Ok(entries.len().min(keep))
}
