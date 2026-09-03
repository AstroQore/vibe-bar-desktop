//! xAI Grok quota — the native `GrokQuotaAdapter` and `GrokWebBillingFetcher`.
//!
//! One POST to `grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig`
//! carrying the bearer token `grok login` wrote into `~/.grok/auth.json`, and
//! one GET to the CLI settings endpoint for the tier the badge shows. The
//! browser-cookie route the native app also accepts waits on a cookie reader.
//!
//! The response is a gRPC-web frame carrying protobuf. There is no generated
//! stub here on purpose: xAI owns the wire format and changes it without
//! notice, so a best-effort scanner pulls the weekly used-percent (a `fixed32`
//! at the shallowest `field 1`) and the next reset (a varint of Unix seconds)
//! out of the bytes, and a layout that no longer carries them is a parse
//! failure rather than a wrong number.

use std::path::Path;

use reqwest::{Client, Url};

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const ACCOUNT_ID: &str = "cli-grok";
const BILLING_URL: &str = "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";
const SETTINGS_URL: &str = "https://cli-chat-proxy.grok.com/v1/settings";
/// xAI's weekly credit window. Naming it keeps a forecast from rejecting a
/// fresh cycle, whose guard wants time-until-reset within the window.
const WEEKLY_WINDOW_SECONDS: i64 = 604_800;

pub async fn fetch(home: &Path, client: &Client) -> Result<AccountQuota, QuotaError> {
    let credential = crate::credentials::grok::load(home)?;
    if credential.is_expired(super::now_unix()) {
        return Err(QuotaError::NeedsLogin);
    }

    // Billing is the quota; settings only enriches the badge, so a settings
    // outage or schema change must never fail the refresh.
    let (billing, settings) = tokio::join!(
        billing(client, &credential.access_token),
        subscription_tier(client, &credential.access_token),
    );
    let snapshot = billing?;
    let plan = settings.ok().flatten().or_else(|| credential.plan_label());

    Ok(AccountQuota {
        account_id: ACCOUNT_ID.to_string(),
        tool: ToolType::Grok,
        buckets: vec![QuotaBucket::new(
            "weekly",
            "Weekly",
            "Weekly",
            snapshot.used_percent,
            snapshot.resets_at,
            Some(WEEKLY_WINDOW_SECONDS),
            None,
        )],
        plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

async fn billing(client: &Client, token: &str) -> Result<Snapshot, QuotaError> {
    let url = Url::parse(BILLING_URL).expect("the built-in Grok URL is valid");
    let response = client
        .post(url)
        .timeout(super::REQUEST_TIMEOUT)
        .header("Authorization", format!("Bearer {token}"))
        .header("Origin", "https://grok.com")
        .header("Referer", "https://grok.com/?_s=usage")
        .header("Accept", "*/*")
        .header("Content-Type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .header("x-user-agent", "connect-es/2.1.1")
        .header("User-Agent", "VibeBar")
        // An empty gRPC-web message: one flag byte and a big-endian length of
        // zero, which is what xAI's own web client sends.
        .body(vec![0x00, 0x00, 0x00, 0x00, 0x00])
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;

    if let Some(error) = super::classify_status(response.status()) {
        return Err(error);
    }
    let header_status = grpc_status_from_headers(response.headers());
    let body = response
        .bytes()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    check_status(header_status)?;
    check_status(grpc_status_from_trailers(&body))?;
    parse_billing(&body, super::now_unix())
}

async fn subscription_tier(client: &Client, token: &str) -> Result<Option<String>, QuotaError> {
    let url = Url::parse(SETTINGS_URL).expect("the built-in Grok settings URL is valid");
    let response = client
        .get(url)
        .timeout(super::REQUEST_TIMEOUT)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .header("User-Agent", "VibeBar")
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    if let Some(error) = super::classify_status(response.status()) {
        return Err(error);
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    Ok(parse_settings(&body))
}

pub fn parse_settings(body: &[u8]) -> Option<String> {
    let root: serde_json::Value = serde_json::from_slice(body).ok()?;
    let raw = root.get("subscription_tier_display")?.as_str()?;
    super::plan_display::grok(raw)
}

// MARK: - gRPC-web framing

/// `(status, message)` from `grpc-status` / `grpc-message`, wherever they came
/// from. `None` means the exchange said nothing about status, which is fine.
type GrpcStatus = Option<(i64, Option<String>)>;

fn check_status(status: GrpcStatus) -> Result<(), QuotaError> {
    let Some((code, message)) = status else {
        return Ok(());
    };
    if code == 0 {
        return Ok(());
    }
    // 16 is UNAUTHENTICATED.
    if code == 16 {
        return Err(QuotaError::NeedsLogin);
    }
    Err(QuotaError::Network(format!(
        "Grok RPC failed: {}",
        message.unwrap_or_else(|| format!("status {code}"))
    )))
}

fn grpc_status_from_headers(headers: &reqwest::header::HeaderMap) -> GrpcStatus {
    let code = headers
        .get("grpc-status")?
        .to_str()
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()?;
    let message = headers
        .get("grpc-message")
        .and_then(|value| value.to_str().ok())
        .map(|raw| percent_decoded(raw.trim()));
    Some((code, message))
}

fn grpc_status_from_trailers(body: &[u8]) -> GrpcStatus {
    let mut code = None;
    let mut message = None;
    for (flags, frame) in frames(body) {
        if flags & 0x80 == 0 {
            continue;
        }
        let Ok(text) = std::str::from_utf8(frame) else {
            continue;
        };
        for line in text.lines().filter(|line| !line.is_empty()) {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = percent_decoded(value.trim());
            match key.trim().to_ascii_lowercase().as_str() {
                "grpc-status" => code = value.parse::<i64>().ok(),
                "grpc-message" => message = Some(value),
                _ => {}
            }
        }
    }
    code.map(|code| (code, message))
}

/// Every length-prefixed frame as `(flags, payload)`. A truncated tail is
/// dropped rather than guessed at.
fn frames(body: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    let mut index = 0;
    while index + 5 <= body.len() {
        let flags = body[index];
        let length = u32::from_be_bytes([
            body[index + 1],
            body[index + 2],
            body[index + 3],
            body[index + 4],
        ]) as usize;
        let start = index + 5;
        let Some(end) = start.checked_add(length).filter(|end| *end <= body.len()) else {
            break;
        };
        out.push((flags, &body[start..end]));
        index = end;
    }
    out
}

fn percent_decoded(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&raw[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

// MARK: - Protobuf scanner

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: Option<f64>,
}

#[derive(Debug, Default)]
struct Scan {
    fixed32: Vec<(Vec<u64>, f32, usize)>,
    varint: Vec<(Vec<u64>, u64)>,
}

pub fn parse_billing(body: &[u8], now_unix: f64) -> Result<Snapshot, QuotaError> {
    let payloads: Vec<&[u8]> = frames(body)
        .into_iter()
        .filter(|(flags, _)| flags & 0x80 == 0)
        .map(|(_, payload)| payload)
        .collect();
    if payloads.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Grok billing response had no data frames".into(),
        ));
    }
    let mut scan = Scan::default();
    for payload in payloads {
        let (nested, _) = scan_protobuf(payload, 0, &[], 0);
        scan.fixed32.extend(nested.fixed32);
        scan.varint.extend(nested.varint);
    }

    // The shallowest `field 1` fixed32 in range is the weekly used-percent;
    // going shallowest keeps this pointed at the top-level usage object
    // instead of a nested bucket that happens to share the field number.
    let percent = scan
        .fixed32
        .iter()
        .filter(|(path, value, _)| {
            path.last() == Some(&1) && value.is_finite() && *value >= 0.0 && *value <= 100.0
        })
        .min_by(|(left, _, left_order), (right, _, right_order)| {
            left.len()
                .cmp(&right.len())
                .then(left_order.cmp(right_order))
        })
        .map(|(_, value, _)| f64::from(*value));

    // Resets are varint Unix seconds. The billing cycle end sits at `[1, 5, 1]`
    // when the payload carries it; otherwise the earliest future timestamp.
    let resets: Vec<(&Vec<u64>, f64)> = scan
        .varint
        .iter()
        .filter(|(_, value)| (1_700_000_000..=2_100_000_000).contains(value))
        .map(|(path, value)| (path, *value as f64))
        .filter(|(_, at)| *at > now_unix)
        .collect();
    let reset = resets
        .iter()
        .filter(|(path, _)| path.as_slice() == [1, 5, 1])
        .map(|(_, at)| *at)
        .fold(None, min_option)
        .or_else(|| resets.iter().map(|(_, at)| *at).fold(None, min_option));

    // An account at exactly 0% omits the default-valued percent. Requiring a
    // known billing marker plus a future reset keeps an unrelated reset-only
    // protobuf from being read as zero usage.
    let legacy_allotment = scan
        .varint
        .iter()
        .any(|(path, _)| path.starts_with(&[1, 6]));
    let weekly_window = scan
        .varint
        .iter()
        .any(|(path, value)| path.as_slice() == [1, 8, 3, 1] && reset == Some(*value as f64));
    let no_usage_yet = percent.is_none()
        && scan.fixed32.is_empty()
        && reset.is_some()
        && (legacy_allotment || weekly_window);

    let used_percent = percent.or(no_usage_yet.then_some(0.0)).ok_or_else(|| {
        QuotaError::ParseFailure("Grok billing protobuf had no usage field".into())
    })?;
    Ok(Snapshot {
        used_percent,
        resets_at: reset,
    })
}

fn min_option(current: Option<f64>, candidate: f64) -> Option<f64> {
    Some(match current {
        Some(current) if current <= candidate => current,
        _ => candidate,
    })
}

/// Walk a protobuf message recording where each scalar sat. Unknown wire
/// types and truncated fields advance one byte rather than aborting, because
/// the goal is to find two known scalars in a message this build does not own.
fn scan_protobuf(bytes: &[u8], depth: usize, path: &[u64], order: usize) -> (Scan, usize) {
    let mut scan = Scan::default();
    let mut index = 0;
    let mut next_order = order;

    while index < bytes.len() {
        let field_start = index;
        let Some(key) = read_varint(bytes, &mut index).filter(|key| *key != 0) else {
            index = field_start + 1;
            continue;
        };
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        let mut field_path = path.to_vec();
        field_path.push(field_number);

        match wire_type {
            0 => match read_varint(bytes, &mut index) {
                Some(value) => scan.varint.push((field_path, value)),
                None => index = field_start + 1,
            },
            1 => {
                if index + 8 > bytes.len() {
                    return (scan, next_order);
                }
                index += 8;
            }
            2 => {
                let Some(length) = read_varint(bytes, &mut index)
                    .filter(|length| *length <= (bytes.len() - index) as u64)
                else {
                    index = field_start + 1;
                    continue;
                };
                let end = index + length as usize;
                if depth < 4 {
                    let (nested, order) =
                        scan_protobuf(&bytes[index..end], depth + 1, &field_path, next_order);
                    scan.fixed32.extend(nested.fixed32);
                    scan.varint.extend(nested.varint);
                    next_order = order;
                }
                index = end;
            }
            5 => {
                if index + 4 > bytes.len() {
                    return (scan, next_order);
                }
                let value = f32::from_le_bytes([
                    bytes[index],
                    bytes[index + 1],
                    bytes[index + 2],
                    bytes[index + 3],
                ]);
                scan.fixed32.push((field_path, value, next_order));
                next_order += 1;
                index += 4;
            }
            _ => index = field_start + 1,
        }
    }
    (scan, next_order)
}

fn read_varint(bytes: &[u8], index: &mut usize) -> Option<u64> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    while *index < bytes.len() && shift < 64 {
        let byte = bytes[*index];
        *index += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Synthetic protobuf builders ──────────────────────────────

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    fn field_varint(number: u64, value: u64) -> Vec<u8> {
        let mut out = varint(number << 3);
        out.extend(varint(value));
        out
    }

    fn field_fixed32(number: u64, value: f32) -> Vec<u8> {
        let mut out = varint((number << 3) | 5);
        out.extend(value.to_le_bytes());
        out
    }

    fn field_message(number: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = varint((number << 3) | 2);
        out.extend(varint(payload.len() as u64));
        out.extend(payload);
        out
    }

    fn data_frame(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x00];
        out.extend((payload.len() as u32).to_be_bytes());
        out.extend(payload);
        out
    }

    fn trailer_frame(text: &str) -> Vec<u8> {
        let mut out = vec![0x80];
        out.extend((text.len() as u32).to_be_bytes());
        out.extend(text.as_bytes());
        out
    }

    const NOW: f64 = 1_780_000_000.0;
    const RESET: u64 = 1_780_600_000;

    /// `{1: {1: 42.5%, 5: {1: reset}}}` — the shape the live endpoint returns.
    fn usage_payload(percent: f32) -> Vec<u8> {
        let cycle = field_varint(1, RESET);
        let mut usage = field_fixed32(1, percent);
        usage.extend(field_message(5, &cycle));
        field_message(1, &usage)
    }

    #[test]
    fn the_weekly_percent_and_its_reset_come_out_of_the_frames() {
        let body = data_frame(&usage_payload(42.5));
        let snapshot = parse_billing(&body, NOW).unwrap();
        assert!((snapshot.used_percent - 42.5).abs() < 0.001);
        assert_eq!(snapshot.resets_at, Some(RESET as f64));
    }

    #[test]
    fn a_nested_bucket_does_not_outrank_the_top_level_percent() {
        let nested = field_message(9, &field_message(2, &field_fixed32(1, 99.0)));
        let mut payload = usage_payload(12.0);
        payload.extend(nested);
        let snapshot = parse_billing(&data_frame(&payload), NOW).unwrap();
        assert!((snapshot.used_percent - 12.0).abs() < 0.001);
    }

    #[test]
    fn an_account_at_zero_needs_a_billing_marker_before_it_reads_as_zero() {
        // Reset alone is not enough: that could be any protobuf.
        let reset_only = field_message(1, &field_message(5, &field_varint(1, RESET)));
        assert!(matches!(
            parse_billing(&data_frame(&reset_only), NOW),
            Err(QuotaError::ParseFailure(_))
        ));
        // The legacy allotment marker under field 6 makes it a billing payload.
        let mut with_allotment = field_message(5, &field_varint(1, RESET));
        with_allotment.extend(field_message(6, &field_varint(1, 100)));
        let snapshot = parse_billing(&data_frame(&field_message(1, &with_allotment)), NOW).unwrap();
        assert_eq!(snapshot.used_percent, 0.0);
        assert_eq!(snapshot.resets_at, Some(RESET as f64));
        // So does the current payload's repeated weekly window at [1, 8, 3, 1].
        let mut with_window = field_message(5, &field_varint(1, RESET));
        with_window.extend(field_message(8, &field_message(3, &field_varint(1, RESET))));
        let snapshot = parse_billing(&data_frame(&field_message(1, &with_window)), NOW).unwrap();
        assert_eq!(snapshot.used_percent, 0.0);
    }

    #[test]
    fn a_reset_already_in_the_past_is_not_the_next_one() {
        let past = field_message(1, &field_message(5, &field_varint(1, 1_700_000_100)));
        let mut payload = past;
        payload.extend(field_fixed32(1, 7.0));
        let snapshot = parse_billing(&data_frame(&payload), NOW).unwrap();
        assert!((snapshot.used_percent - 7.0).abs() < 0.001);
        assert_eq!(snapshot.resets_at, None);
    }

    #[test]
    fn a_response_with_no_data_frames_is_a_parse_failure() {
        assert!(matches!(
            parse_billing(&[], NOW),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse_billing(&trailer_frame("grpc-status: 0\r\n"), NOW),
            Err(QuotaError::ParseFailure(_))
        ));
    }

    #[test]
    fn an_unauthenticated_trailer_asks_for_a_login() {
        let mut body = data_frame(&usage_payload(1.0));
        body.extend(trailer_frame(
            "grpc-status:16\r\ngrpc-message:token%20expired\r\n",
        ));
        assert!(matches!(
            check_status(grpc_status_from_trailers(&body)),
            Err(QuotaError::NeedsLogin)
        ));
        let mut failed = data_frame(&usage_payload(1.0));
        failed.extend(trailer_frame(
            "grpc-status:13\r\ngrpc-message:internal%20error\r\n",
        ));
        match check_status(grpc_status_from_trailers(&failed)) {
            Err(QuotaError::Network(message)) => assert!(message.contains("internal error")),
            other => panic!("{other:?}"),
        }
        let mut ok = data_frame(&usage_payload(1.0));
        ok.extend(trailer_frame("grpc-status:0\r\n"));
        check_status(grpc_status_from_trailers(&ok)).unwrap();
        check_status(grpc_status_from_trailers(&data_frame(&[]))).unwrap();
    }

    #[test]
    fn the_settings_endpoint_supplies_the_tier_badge() {
        assert_eq!(
            parse_settings(br#"{"subscription_tier_display": "SUPER_GROK_HEAVY"}"#).as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(parse_settings(br#"{"other": 1}"#), None);
        assert_eq!(parse_settings(b"<html>"), None);
    }
}
