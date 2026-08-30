//! Kiro quota via the local `kiro-cli` (synthetic-parser-first adapter).

use crate::{
    error::QuotaError,
    model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType},
};
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_OUTPUT: usize = 1024 * 1024;

pub fn find_cli(home: &Path, env: &[(String, String)]) -> Option<PathBuf> {
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let mut dirs: Vec<PathBuf> = std::env::split_paths(path).collect();
    dirs.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
            .into_iter()
            .map(PathBuf::from),
    );
    dirs.push(home.join(".local/bin"));
    for dir in dirs {
        for name in if cfg!(windows) {
            vec!["kiro-cli.exe", "kiro-cli"]
        } else {
            vec!["kiro-cli"]
        } {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub async fn fetch(home: &Path, env: &[(String, String)]) -> Result<AccountQuota, QuotaError> {
    let cli = find_cli(home, env).ok_or(QuotaError::NoCredential)?;
    let whoami = run(&cli, &["whoami"], Duration::from_secs(8)).await?;
    validate_whoami(&whoami.text, whoami.success)?;
    let usage = run(
        &cli,
        &["chat", "--no-interactive", "/usage"],
        Duration::from_secs(20),
    )
    .await?;
    let (buckets, plan) = parse(&usage.text, super::now_unix())?;
    Ok(AccountQuota {
        account_id: "misc-kiro".into(),
        tool: ToolType::Kiro,
        buckets,
        plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

struct CommandOutput {
    success: bool,
    text: String,
}

async fn run(cli: &Path, args: &[&str], timeout: Duration) -> Result<CommandOutput, QuotaError> {
    let mut command = tokio::process::Command::new(cli);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if std::env::var_os("TERM").is_none() {
        command.env("TERM", "dumb");
    }
    let mut child = command
        .spawn()
        .map_err(|error| QuotaError::Network(format!("Kiro CLI launch failed: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| QuotaError::Network("Kiro CLI stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| QuotaError::Network("Kiro CLI stderr unavailable".into()))?;
    let completed = tokio::time::timeout(timeout, async {
        let wait = async {
            child
                .wait()
                .await
                .map_err(|error| QuotaError::Network(format!("Kiro CLI wait failed: {error}")))
        };
        tokio::try_join!(wait, read_limited(stdout), read_limited(stderr))
    })
    .await;
    let (status, mut stdout, stderr) = match completed {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(QuotaError::TimedOut);
        }
    };
    stdout.extend_from_slice(&stderr);
    if stdout.len() > MAX_OUTPUT {
        return Err(QuotaError::ParseFailure(
            "Kiro output exceeded limit".into(),
        ));
    }
    Ok(CommandOutput {
        success: status.success(),
        text: String::from_utf8_lossy(&stdout).into_owned(),
    })
}

async fn read_limited<R: AsyncRead + Unpin>(reader: R) -> Result<Vec<u8>, QuotaError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_OUTPUT as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| QuotaError::Network(format!("Kiro CLI output read failed: {error}")))?;
    if bytes.len() > MAX_OUTPUT {
        return Err(QuotaError::ParseFailure(
            "Kiro output exceeded limit".into(),
        ));
    }
    Ok(bytes)
}

fn validate_whoami(output: &str, success: bool) -> Result<(), QuotaError> {
    let text = strip_ansi(output).to_ascii_lowercase();
    if text.contains("not logged in")
        || text.contains("login required")
        || text.contains("kiro-cli login")
        || text.contains("login --use-device-flow")
        || text.contains("failed to initialize auth portal")
        || text.contains("oauth error")
    {
        return Err(QuotaError::NeedsLogin);
    }
    if !success || text.trim().is_empty() {
        return Err(QuotaError::NoCredential);
    }
    Ok(())
}

pub fn parse(output: &str, now: f64) -> Result<(Vec<QuotaBucket>, Option<String>), QuotaError> {
    let text = strip_ansi(output);
    let lower = text.to_ascii_lowercase();
    if lower.contains("kiro-cli login")
        || lower.contains("login --use-device-flow")
        || lower.contains("not logged in")
        || lower.contains("login required")
        || lower.contains("failed to initialize auth portal")
        || lower.contains("oauth error")
    {
        return Err(QuotaError::NeedsLogin);
    }
    if lower.contains("could not retrieve usage") || lower.contains("dispatch failure") {
        return Err(QuotaError::ParseFailure("Kiro CLI warning output".into()));
    }
    let plan = text
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Plan:")
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            text.lines()
                .find(|line| line.contains("Estimated Usage") && line.contains('|'))
                .and_then(|line| line.split('|').next_back())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        });
    let mut buckets = Vec::new();
    if let Some((used, total, percent)) = parse_usage(&text) {
        let pct = percent.unwrap_or_else(|| {
            if total > 0.0 {
                used / total * 100.0
            } else {
                0.0
            }
        });
        buckets.push(QuotaBucket::new(
            "kiro.credits",
            "Credits",
            "Credits",
            pct,
            parse_reset(&text, now),
            None,
            Some(format!("{used} / {total} covered")),
        ));
    } else if lower.contains("managed by admin") || lower.contains("managed by organization") {
        buckets.push(QuotaBucket::new(
            "kiro.credits",
            "Credits",
            "Credits",
            0.0,
            None,
            None,
            None,
        ));
    }
    if let Some((used, total, days)) = parse_bonus(&text) {
        buckets.push(QuotaBucket::new(
            "kiro.bonus",
            "Bonus Credits",
            "Bonus",
            if total > 0.0 {
                used / total * 100.0
            } else {
                0.0
            },
            days.map(|d| now + d as f64 * 86400.0),
            None,
            Some(format!("{used} / {total} credits")),
        ));
    }
    if buckets.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Kiro usage output was not recognized".into(),
        ));
    }
    Ok((buckets, plan))
}

fn parse_usage(text: &str) -> Option<(f64, f64, Option<f64>)> {
    let line = text
        .lines()
        .find(|line| line.contains(" of ") && line.contains("covered in plan"))?;
    let mut nums = line
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|v| !v.is_empty())
        .filter_map(|v| v.parse().ok());
    Some((
        nums.next()?,
        nums.next()?,
        text.split('%').next_back().and_then(|_| {
            text.lines()
                .find_map(|l| l.trim().strip_suffix('%')?.trim().parse().ok())
        }),
    ))
}
fn parse_bonus(text: &str) -> Option<(f64, f64, Option<u64>)> {
    let line = text
        .lines()
        .find(|line| line.to_ascii_lowercase().contains("bonus credits:"))?;
    let nums: Vec<f64> = line
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|v| !v.is_empty())
        .filter_map(|v| v.parse().ok())
        .collect();
    Some((
        *nums.first()?,
        *nums.get(1)?,
        line.split("expires in")
            .nth(1)
            .and_then(|v| v.split_whitespace().next()?.parse().ok()),
    ))
}
fn parse_reset(text: &str, now: f64) -> Option<f64> {
    let line = text
        .lines()
        .find(|l| l.to_ascii_lowercase().contains("resets on"))?;
    let lower = line.to_ascii_lowercase();
    let raw = lower.split("resets on").nth(1)?.split_whitespace().next()?;
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(
            DateTime::<Utc>::from_naive_utc_and_offset(date.and_time(NaiveTime::MIN), Utc)
                .timestamp() as f64,
        );
    }
    let parts: Vec<u32> = raw.split('/').filter_map(|v| v.parse().ok()).collect();
    if parts.len() != 2 {
        return None;
    }
    let year = DateTime::<Utc>::from_timestamp(now as i64, 0)?.year();
    let date = NaiveDate::from_ymd_opt(year, parts[0], parts[1])?;
    let timestamp = DateTime::<Utc>::from_naive_utc_and_offset(date.and_time(NaiveTime::MIN), Utc)
        .timestamp() as f64;
    if timestamp >= now {
        Some(timestamp)
    } else {
        Some(
            DateTime::<Utc>::from_naive_utc_and_offset(
                NaiveDate::from_ymd_opt(year + 1, parts[0], parts[1])?.and_time(NaiveTime::MIN),
                Utc,
            )
            .timestamp() as f64,
        )
    }
}
fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_and_discovery_are_noninteractive() {
        let env = vec![("PATH".into(), "/missing".into())];
        assert!(find_cli(Path::new("/synthetic"), &env).is_none());
    }
    #[test]
    fn parses_credits_bonus_and_managed() {
        let (b,p)=parse("Plan: KIRO PRO\nCredits (40.00 of 50 covered in plan), resets on 02/01\n80%\nBonus credits: 5.00/10 credits used, expires in 7 days", 1_715_000_000.0).unwrap();
        assert_eq!(p.as_deref(), Some("KIRO PRO"));
        assert_eq!(
            b.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
            vec!["kiro.credits", "kiro.bonus"]
        );
        assert!(b[0].reset_at.is_some_and(|reset| reset > 1_715_000_000.0));
        let (managed, _) =
            parse("Plan: Q Developer Pro\nmanaged by admin", 1_715_000_000.0).unwrap();
        assert_eq!(managed[0].used_percent, 0.0);
    }
    #[test]
    fn auth_and_parse_fail_closed() {
        assert_eq!(
            validate_whoami("not logged in", true),
            Err(QuotaError::NeedsLogin)
        );
        assert!(matches!(
            parse("garbage", 0.0),
            Err(QuotaError::ParseFailure(_))
        ));
    }
}
