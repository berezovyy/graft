use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use walkdir::WalkDir;
use std::time::{SystemTime, UNIX_EPOCH};

/// Return the current UTC time as an RFC 3339 string (e.g. "2026-03-22T14:30:00Z").
pub fn now_rfc3339() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let (y, mo, d) = days_to_date(days);

    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Parse an RFC 3339 "...Z" timestamp to epoch seconds. Returns None on failure.
pub fn parse_rfc3339_secs(s: &str) -> Option<u64> {
    let s = s.trim_end_matches('Z');
    let (date_part, time_part) = s.split_once('T')?;
    let mut date_iter = date_part.split('-');
    let y: u64 = date_iter.next()?.parse().ok()?;
    let mo: u64 = date_iter.next()?.parse().ok()?;
    let d: u64 = date_iter.next()?.parse().ok()?;

    let time_part = time_part.split('.').next()?; // ignore fractional
    let mut time_iter = time_part.split(':');
    let h: u64 = time_iter.next()?.parse().ok()?;
    let m: u64 = time_iter.next()?.parse().ok()?;
    let s_val: u64 = time_iter.next()?.parse().ok()?;

    let days = date_to_days(y, mo, d)?;
    Some(days * 86400 + h * 3600 + m * 60 + s_val)
}

/// Epoch seconds for "now".
pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Convert days since Unix epoch to (year, month, day).
/// Algorithm from Howard Hinnant's chrono-compatible date library.
fn days_to_date(epoch_days: u64) -> (u64, u64, u64) {
    let z = epoch_days + 719468;
    let era = z / 146097;
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

fn date_to_days(y: u64, m: u64, d: u64) -> Option<u64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y.wrapping_sub(1) } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// Compute the difference in seconds between now and a parsed RFC 3339 timestamp.
pub fn age_seconds(rfc3339: &str) -> Option<u64> {
    let then = parse_rfc3339_secs(rfc3339)?;
    let now = now_epoch_secs();
    Some(now.saturating_sub(then))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub base: PathBuf,
    pub upper: PathBuf,
    pub work: PathBuf,
    pub merged: PathBuf,
    pub parent: Option<String>,
    pub created: String,
    pub session: Option<String>,
    #[serde(default)]
    pub tmpfs: bool,
    #[serde(default)]
    pub tmpfs_size: Option<String>,
    #[serde(default)]
    pub ephemeral: bool,
}

pub fn graft_home() -> PathBuf {
    if let Ok(home) = std::env::var("GRAFT_HOME") {
        PathBuf::from(home)
    } else {
        let home = std::env::var("HOME").expect("HOME environment variable not set");
        PathBuf::from(home).join(".graft")
    }
}

impl Workspace {
    pub fn new(name: &str, base: PathBuf, parent: Option<String>) -> Self {
        let root = graft_home().join(name);
        Self {
            name: name.to_string(),
            base,
            upper: root.join("upper"),
            work: root.join("work"),
            merged: root.join("merged"),
            parent,
            created: now_rfc3339(),
            session: None,
            tmpfs: false,
            tmpfs_size: None,
            ephemeral: false,
        }
    }

    pub fn dirs_exist(&self) -> bool {
        self.upper.exists() && self.work.exists() && self.merged.exists()
    }

    /// Resolve a relative path against the base directory.
    pub fn lower_path(&self, rel: &std::path::Path) -> PathBuf {
        self.base.join(rel)
    }

    /// Count regular files in the upper directory, excluding OverlayFS whiteout markers.
    pub fn count_upper_files(&self) -> usize {
        if !self.upper.exists() {
            return 0;
        }
        WalkDir::new(&self.upper)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|name| !name.starts_with(".wh."))
            })
            .count()
    }
}
