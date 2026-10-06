use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::{create_private_dir, durable_create_new, durable_replace};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

/// Room kept below the byte cap so truncating a long detail never cuts the
/// count lines, which would silently reset the count on the next repeat.
const COUNTER_BYTES: usize = 128;

/// Serializes the read-count-replace in `write_private_diagnostic`, so two
/// threads reporting the same failure never lose an occurrence.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Writes `<code>-<id>.txt` holding `code=`, `detail=`, `count=`,
/// `first_seen_epoch_seconds=` and `last_seen_epoch_seconds=` lines.
///
/// The id is derived from the code and redacted detail, so a failure that
/// repeats (a background loop retrying every 250 ms) updates one file's count
/// instead of adding a file per occurrence. Files written before this format
/// carried a random id and no count; they are left as they are.
pub fn write_private_diagnostic(directory: &Path, code: &str, detail: &str) -> io::Result<PathBuf> {
    create_private_dir(directory)?;
    let safe_code: String = code
        .chars()
        .filter(|character| {
            character.is_ascii_lowercase() || *character == '_' || character.is_ascii_digit()
        })
        .take(64)
        .collect();
    let head = bounded_head(code, &redact(detail));
    let digest = Sha256::digest(head.as_bytes());
    let id = Uuid::from_bytes(digest[..16].try_into().expect("sha256 is 32 bytes"));
    let file_name = format!(
        "{}-{}.txt",
        if safe_code.is_empty() {
            "diagnostic"
        } else {
            &safe_code
        },
        id
    );
    let path = directory.join(file_name);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);

    let _guard = WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = match fs::read(&path) {
        Ok(bytes) => Some(previous_occurrences(&bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    // An unreadable earlier count restarts at 1 rather than failing the write.
    let (count, first_seen) = match previous.flatten() {
        Some((count, first_seen)) => (count.saturating_add(1), first_seen.unwrap_or(now)),
        None => (1, now),
    };
    let body = format!(
        "{head}count={count}\nfirst_seen_epoch_seconds={first_seen}\nlast_seen_epoch_seconds={now}\n"
    );
    if previous.is_some() {
        durable_replace(&path, body.as_bytes())?;
    } else {
        durable_create_new(&path, body.as_bytes())?;
    }
    Ok(path)
}

fn bounded_head(code: &str, redacted: &str) -> String {
    let mut head = format!("code={code}\ndetail={redacted}");
    let limit = MAX_DIAGNOSTIC_BYTES - COUNTER_BYTES - 1;
    if head.len() > limit {
        let mut end = limit;
        while !head.is_char_boundary(end) {
            end -= 1;
        }
        head.truncate(end);
    }
    head.push('\n');
    head
}

/// Reads the count and first-seen time from an existing file. The detail line
/// cannot start a line with `count=`: redaction removes any token with `=`.
fn previous_occurrences(bytes: &[u8]) -> Option<(u64, Option<u64>)> {
    let text = String::from_utf8_lossy(bytes);
    let mut count = None;
    let mut first_seen = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("count=") {
            count = value.parse().ok();
        } else if let Some(value) = line.strip_prefix("first_seen_epoch_seconds=") {
            first_seen = value.parse().ok();
        }
    }
    count.map(|count| (count, first_seen))
}

fn redact(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            if token.contains('/')
                || token.contains('\\')
                || token.contains('@')
                || token.contains('=')
            {
                "[redacted]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn diagnostic_is_private(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::metadata(path)?.permissions().mode() & 0o777 == 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn removes_paths_and_environment_values() {
        let temp = TempDir::new().unwrap();
        let path = write_private_diagnostic(
            temp.path(),
            "runtime_missing",
            "worker /private/person/file TOKEN=secret person@example.com missing",
        )
        .unwrap();
        let body = fs::read_to_string(&path).unwrap();
        assert!(!body.contains("/private/person"));
        assert!(!body.contains("secret"));
        assert!(!body.contains("example.com"));
        assert!(diagnostic_is_private(&path).unwrap());
    }

    #[test]
    fn repeated_identical_diagnostics_stay_one_counted_file() {
        let temp = TempDir::new().unwrap();
        // A file the previous writer left behind: never touched or deleted.
        let legacy = temp
            .path()
            .join("transcription_queue_recovery_failed-6f1c1a52-0b1e-4c43-9a8e-1d2f3a4b5c6d.txt");
        let legacy_body = "code=transcription_queue_recovery_failed\ndetail=transcription queue storage is invalid\n";
        fs::write(&legacy, legacy_body).unwrap();

        let mut last = None;
        for _ in 0..1_000 {
            last = Some(
                write_private_diagnostic(
                    temp.path(),
                    "transcription_queue_recovery_failed",
                    "transcription queue storage is invalid",
                )
                .unwrap(),
            );
        }
        let path = last.unwrap();

        // Every entry counts, dotfiles included, so a leaked temporary fails.
        let entries = fs::read_dir(temp.path()).unwrap().count();
        assert_eq!(entries, 2, "{entries} entries after 1,000 identical writes");
        assert_eq!(fs::read_to_string(&legacy).unwrap(), legacy_body);

        let body = fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = body.lines().collect();
        assert_eq!(lines[0], "code=transcription_queue_recovery_failed");
        assert_eq!(lines[1], "detail=transcription queue storage is invalid");
        assert!(lines.contains(&"count=1000"), "{body}");
        assert!(diagnostic_is_private(&path).unwrap());

        let other = write_private_diagnostic(
            temp.path(),
            "transcription_queue_recovery_failed",
            "a different failure",
        )
        .unwrap();
        assert_ne!(other, path);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
    }

    #[test]
    fn a_repeat_keeps_the_first_seen_time() {
        let temp = TempDir::new().unwrap();
        let path = write_private_diagnostic(temp.path(), "worker_failed", "exit 1").unwrap();
        let body = fs::read_to_string(&path).unwrap();
        let backdated: String = body
            .lines()
            .map(|line| {
                if line.starts_with("first_seen_epoch_seconds=") {
                    "first_seen_epoch_seconds=1\n".to_string()
                } else {
                    format!("{line}\n")
                }
            })
            .collect();
        fs::write(&path, backdated).unwrap();

        write_private_diagnostic(temp.path(), "worker_failed", "exit 1").unwrap();
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.lines().any(|line| line == "first_seen_epoch_seconds=1"), "{body}");
        assert!(body.lines().any(|line| line == "count=2"), "{body}");
    }

    #[test]
    fn an_oversized_detail_keeps_its_count_inside_the_byte_cap() {
        let temp = TempDir::new().unwrap();
        let detail = "é".repeat(MAX_DIAGNOSTIC_BYTES);
        write_private_diagnostic(temp.path(), "worker_failed", &detail).unwrap();
        let path = write_private_diagnostic(temp.path(), "worker_failed", &detail).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.len() <= MAX_DIAGNOSTIC_BYTES);
        let body = String::from_utf8(bytes).expect("truncation keeps whole characters");
        assert!(body.lines().any(|line| line == "count=2"));
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}
