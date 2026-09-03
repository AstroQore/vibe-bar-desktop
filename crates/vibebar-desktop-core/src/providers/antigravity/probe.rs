//! Finding the AntiGravity language server on this machine and talking to it
//! — the native `AntigravityLanguageServerClient`.
//!
//! The server is discovered from the process list, because its command line
//! is where the CSRF token lives; its listening ports come from `lsof`; and
//! the request goes to loopback only. A machine commonly runs more than one
//! server (the IDE's, and a CLI session's), each owning different state, so
//! every candidate is tried.
//!
//! **The loopback client is deliberately separate from the app's.** The
//! language server presents a self-signed certificate, so certificate
//! verification is off for it — which is only safe because it is used for
//! `https://127.0.0.1:<port>` and nothing else, and proxies are disabled so
//! the CSRF token cannot be sent anywhere but this machine.

use std::time::Duration;

use crate::error::QuotaError;

const TIMEOUT: Duration = Duration::from_secs(8);
/// Every AntiGravity server's binary contains this. Version 1.x shipped
/// `language_server_macos`, 2.x renamed it to `language_server`; the loose
/// match covers both and the AntiGravity-specific check below narrows it.
const PROCESS_NAME_SUBSTRING: &str = "language_server";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProcess {
    pub pid: i32,
    /// Sensitive: never log. It is the whole authorisation.
    pub csrf_token: String,
    pub extension_port: Option<u16>,
    pub extension_csrf_token: Option<String>,
}

/// A loopback address the server was seen listening on. Which one matters:
/// a server bound only to `[::1]` is unreachable through `127.0.0.1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loopback {
    V4,
    V6,
}

impl Loopback {
    /// The host as it goes into a URL, brackets and all.
    fn host(self) -> &'static str {
        match self {
            Loopback::V4 => "127.0.0.1",
            Loopback::V6 => "[::1]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: &'static str,
    pub host: Loopback,
    pub port: u16,
    pub csrf_token: String,
}

pub struct LocalClient {
    client: reqwest::Client,
}

impl LocalClient {
    pub fn new() -> Result<Self, QuotaError> {
        let client = reqwest::Client::builder()
            // The language server's certificate is self-signed and generated
            // per install. This client only ever addresses 127.0.0.1, and
            // no_proxy keeps the token off any proxy the environment names.
            .danger_accept_invalid_certs(true)
            // A redirect would take the token, and the relaxed certificate
            // check, somewhere that is not this machine. There is nowhere for
            // a loopback RPC to legitimately redirect to.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| {
                QuotaError::Unknown(format!("Antigravity loopback client: {error}"))
            })?;
        Ok(Self { client })
    }

    pub fn timeout(&self) -> Duration {
        TIMEOUT
    }

    pub async fn post(
        &self,
        endpoint: &Endpoint,
        path: &str,
        body: &'static [u8],
    ) -> Result<Vec<u8>, QuotaError> {
        let url = format!(
            "{}://{}:{}{}",
            endpoint.scheme,
            endpoint.host.host(),
            endpoint.port,
            path
        );
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", &endpoint.csrf_token)
            .body(body)
            .send()
            .await
            .map_err(|error| super::super::classify_transport(&error))?;
        if let Some(error) = super::super::classify_status(response.status()) {
            return Err(error);
        }
        Ok(response
            .bytes()
            .await
            .map_err(|error| super::super::classify_transport(&error))?
            .to_vec())
    }
}

/// Every candidate endpoint of every AntiGravity server running here.
pub async fn connected_endpoints(timeout: &Duration) -> Result<Vec<Endpoint>, QuotaError> {
    let processes = detect(timeout).await?;
    let mut endpoints = Vec::new();
    let mut port_error = None;
    for process in processes {
        let ports = match listening_ports(process.pid, timeout).await {
            Ok(ports) => ports,
            Err(error) => {
                // A missing or blocked lsof is not "no ports yet", and
                // waiting will not fix it. Keep the reason in case no
                // process yields an endpoint.
                port_error.get_or_insert(error);
                Vec::new()
            }
        };
        endpoints.extend(candidates(&process, &ports));
    }
    if endpoints.is_empty() {
        return Err(port_error.unwrap_or_else(|| QuotaError::ParseFailure(
            "Antigravity is running but no listening ports found yet — wait a few seconds and retry".into(),
        )));
    }
    Ok(endpoints)
}

async fn detect(timeout: &Duration) -> Result<Vec<ServerProcess>, QuotaError> {
    if !cfg!(unix) {
        // The token lives in the command line, which needs a process listing
        // this build does not have on Windows yet. Saying so beats guessing.
        return Err(QuotaError::NotImplemented);
    }
    // `-x` without `-a`: this user's processes, including those with no
    // terminal, and nobody else's — another account's CSRF token is not ours
    // to use, and its quota is not ours to publish. `-ww` twice because
    // Darwin otherwise truncates the command to the terminal width, and the
    // AntiGravity path alone can run past the `--csrf_token` that follows.
    let output = run("/bin/ps", &["-xww", "-o", "pid=,command="], timeout)
        .await
        .map_err(|error| QuotaError::Unknown(format!("Could not list processes: {error}")))?;
    let processes = parse_processes(&output);
    if processes.is_empty() {
        return Err(if saw_antigravity(&output) {
            QuotaError::ParseFailure(
                "Antigravity is running but its CSRF token is missing — restart Antigravity and retry".into(),
            )
        } else {
            QuotaError::NoCredential
        });
    }
    Ok(processes)
}

/// One `ServerProcess` per AntiGravity language server that exposes a token.
/// Pure, so multi-server detection is testable without a process list.
pub fn parse_processes(ps_output: &str) -> Vec<ServerProcess> {
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, command) = split_process_line(line)?;
            is_antigravity(&command.to_lowercase()).then_some(())?;
            let csrf_token = flag(command, "--csrf_token")?;
            Some(ServerProcess {
                pid,
                csrf_token,
                extension_port: flag(command, "--extension_server_port")
                    .and_then(|raw| raw.parse().ok()),
                extension_csrf_token: flag(command, "--extension_server_csrf_token"),
            })
        })
        .collect()
}

/// Whether any AntiGravity server is there at all, even without a readable
/// token — what tells "restart it" apart from "not running".
pub fn saw_antigravity(ps_output: &str) -> bool {
    ps_output.lines().any(|line| {
        split_process_line(line).is_some_and(|(_, command)| is_antigravity(&command.to_lowercase()))
    })
}

fn split_process_line(line: &str) -> Option<(i32, &str)> {
    let line = line.trim_start();
    let end = line.find(char::is_whitespace)?;
    let pid = line[..end].parse().ok()?;
    let command = line[end..].trim_start();
    (!command.is_empty()).then_some((pid, command))
}

/// A `language_server` binary plus something that names AntiGravity, so
/// another vendor's language server is not mistaken for one.
fn is_antigravity(lowercased_command: &str) -> bool {
    if !lowercased_command.contains(PROCESS_NAME_SUBSTRING) {
        return false;
    }
    (lowercased_command.contains("--app_data_dir") && lowercased_command.contains("antigravity"))
        || lowercased_command.contains("/antigravity/")
        || lowercased_command.contains("\\antigravity\\")
}

/// `--flag value` or `--flag=value`, up to the next space.
fn flag(command: &str, name: &str) -> Option<String> {
    let mut rest = command;
    loop {
        let start = rest.find(name)?;
        let after = &rest[start + name.len()..];
        let value = after.trim_start_matches(['=', ' ', '\t']);
        // Only a real separator counts, so `--csrf_token_extra` is not read
        // as `--csrf_token`.
        if value.len() < after.len() {
            let end = value.find(char::is_whitespace).unwrap_or(value.len());
            if end > 0 {
                return Some(value[..end].to_string());
            }
        }
        rest = &rest[start + name.len()..];
    }
}

async fn listening_ports(pid: i32, timeout: &Duration) -> Result<Vec<(Loopback, u16)>, QuotaError> {
    let lsof = ["/usr/sbin/lsof", "/usr/bin/lsof"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .ok_or_else(|| {
            QuotaError::Unknown("`lsof` not available; cannot probe Antigravity ports".into())
        })?;
    let pid = pid.to_string();
    let output = run(
        lsof,
        &["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", &pid],
        timeout,
    )
    .await
    .map_err(|error| QuotaError::Unknown(format!("lsof failed: {error}")))?;
    let ports = parse_listening_sockets(&output);
    if ports.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Antigravity is running but no listening ports found yet — wait a few seconds and retry".into(),
        ));
    }
    Ok(ports)
}

/// The listening sockets out of `lsof -nP -iTCP -sTCP:LISTEN`, keeping which
/// loopback address each was on — a server bound only to `[::1]` cannot be
/// reached through `127.0.0.1`. Sorted, deduplicated, IPv4 first so the
/// common case is tried first.
pub fn parse_listening_sockets(output: &str) -> Vec<(Loopback, u16)> {
    let mut sockets: Vec<(Loopback, u16)> = output
        .lines()
        .filter(|line| line.contains("(LISTEN)"))
        .filter_map(|line| {
            let before = line.split("(LISTEN)").next()?.trim_end();
            let socket = before.rsplit(char::is_whitespace).next()?;
            let (address, port) = socket.rsplit_once(':')?;
            let port = port.parse().ok()?;
            // `*` and `[::1]`/`[::]` are the v6 shapes lsof prints; anything
            // in brackets is v6, a bare address is v4.
            let family = if address.starts_with('[') || address == "*" {
                Loopback::V6
            } else {
                Loopback::V4
            };
            Some((family, port))
        })
        .collect();
    // `*` binds both families, so a wildcard port is worth trying on each.
    let wildcards: Vec<u16> = output
        .lines()
        .filter(|line| line.contains("(LISTEN)"))
        .filter_map(|line| {
            let before = line.split("(LISTEN)").next()?.trim_end();
            let socket = before.rsplit(char::is_whitespace).next()?;
            let (address, port) = socket.rsplit_once(':')?;
            (address == "*").then(|| port.parse().ok())?
        })
        .collect();
    sockets.extend(wildcards.into_iter().map(|port| (Loopback::V4, port)));
    sockets.sort_unstable_by_key(|(family, port)| (*port, matches!(family, Loopback::V6)));
    sockets.dedup();
    sockets
}

/// Every shape worth trying for one server: each listening port over HTTPS,
/// then the extension server's plain-HTTP port with its own token and with
/// the main one, since builds differ on which it accepts.
pub fn candidates(process: &ServerProcess, sockets: &[(Loopback, u16)]) -> Vec<Endpoint> {
    let mut endpoints: Vec<Endpoint> = sockets
        .iter()
        .map(|(host, port)| Endpoint {
            scheme: "https",
            host: *host,
            port: *port,
            csrf_token: process.csrf_token.clone(),
        })
        .collect();
    if let Some(port) = process.extension_port {
        // lsof did not name this one — it comes from the command line — so
        // both loopback addresses are worth a try.
        for host in [Loopback::V4, Loopback::V6] {
            for token in [
                process.extension_csrf_token.as_ref(),
                Some(&process.csrf_token),
            ]
            .into_iter()
            .flatten()
            {
                let candidate = Endpoint {
                    scheme: "http",
                    host,
                    port,
                    csrf_token: token.clone(),
                };
                if !endpoints.contains(&candidate) {
                    endpoints.push(candidate);
                }
            }
        }
    }
    endpoints
}

/// Run a read-only helper and return its stdout, bounded in time and size.
async fn run(binary: &str, args: &[&str], timeout: &Duration) -> Result<String, String> {
    const MAX_OUTPUT: usize = 4 * 1024 * 1024;
    const MAX_STDERR: usize = 4 * 1024;
    let mut child = tokio::process::Command::new(binary)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("stderr unavailable")?;
    let collected = tokio::time::timeout(*timeout, async {
        use tokio::io::AsyncReadExt;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let read = tokio::io::BufReader::new(stdout)
            .take(MAX_OUTPUT as u64)
            .read_to_end(&mut out)
            .await;
        let _ = tokio::io::BufReader::new(stderr)
            .take(MAX_STDERR as u64)
            .read_to_end(&mut err)
            .await;
        let status = child.wait().await;
        (read, status, out, err)
    })
    .await;
    let (read, status, out, err) = match collected {
        Ok(collected) => collected,
        Err(_) => return Err("timed out".into()),
    };
    read.map_err(|error| error.to_string())?;
    // A helper that ran but refused tells us nothing about whether a server
    // is there. Treating its empty output as "no servers" would turn a
    // blocked `lsof` into "wait a few seconds", which waiting cannot fix.
    match status {
        Ok(status) if status.success() => Ok(String::from_utf8_lossy(&out).into_owned()),
        Ok(status) => Err(format!(
            "exited with {}{}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or("a signal".into()),
            first_line(&err)
                .map(|line| format!(": {line}"))
                .unwrap_or_default()
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// The first line of a helper's stderr, trimmed and bounded, for an error
/// message. Nothing here is a path this app chose, so it is safe to show.
fn first_line(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(line.chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = concat!(
        "  501 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos --csrf_token abc123 --extension_server_port 5500 --extension_server_csrf_token ext999\n",
        "  502 /usr/local/bin/language_server --app_data_dir /Users/example/.antigravity --csrf_token=def456\n",
        "  503 /opt/othervendor/language_server --csrf_token should-be-ignored\n",
        "  504 /Applications/Antigravity.app/.../language_server_macos --no-token-here\n",
        "  bad line without a pid\n",
    );

    #[test]
    fn every_antigravity_server_with_a_token_is_found() {
        let processes = parse_processes(PS);
        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].pid, 501);
        assert_eq!(processes[0].csrf_token, "abc123");
        assert_eq!(processes[0].extension_port, Some(5500));
        assert_eq!(processes[0].extension_csrf_token.as_deref(), Some("ext999"));
        // `--flag=value` reads the same as `--flag value`.
        assert_eq!(processes[1].pid, 502);
        assert_eq!(processes[1].csrf_token, "def456");
        assert_eq!(processes[1].extension_port, None);
        // Another vendor's language server is not AntiGravity's.
        assert!(!processes.iter().any(|p| p.pid == 503));
    }

    #[test]
    fn a_server_without_a_token_still_counts_as_running() {
        assert!(saw_antigravity(PS));
        let tokenless = "  504 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos --nope\n";
        assert!(parse_processes(tokenless).is_empty());
        assert!(saw_antigravity(tokenless));
        assert!(!saw_antigravity(
            "  503 /opt/othervendor/language_server --csrf_token x\n"
        ));
        assert!(!saw_antigravity(""));
    }

    #[test]
    fn a_flag_is_not_matched_by_a_longer_one_that_starts_the_same() {
        let line = "  1 /x/antigravity/language_server --csrf_token_extra nope --csrf_token yes";
        let processes = parse_processes(line);
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].csrf_token, "yes");
    }

    #[test]
    fn listening_sockets_keep_their_address_family() {
        let output = concat!(
            "COMMAND   PID USER   FD  TYPE            DEVICE SIZE/OFF NODE NAME\n",
            "language_ 501 me    30u  IPv4 0x1              0t0  TCP 127.0.0.1:9100 (LISTEN)\n",
            "language_ 501 me    31u  IPv6 0x2              0t0  TCP [::1]:9100 (LISTEN)\n",
            "language_ 501 me    32u  IPv6 0x3              0t0  TCP [::1]:8080 (LISTEN)\n",
            "language_ 501 me    33u  IPv4 0x4              0t0  TCP 127.0.0.1:7000->1.2.3.4:443 (ESTABLISHED)\n",
        );
        assert_eq!(
            parse_listening_sockets(output),
            vec![
                (Loopback::V6, 8080),
                (Loopback::V4, 9100),
                (Loopback::V6, 9100)
            ]
        );
        // A wildcard bind is worth trying on both families.
        let wildcard = "language_ 501 me 30u IPv6 0x1 0t0 TCP *:9100 (LISTEN)\n";
        assert_eq!(
            parse_listening_sockets(wildcard),
            vec![(Loopback::V4, 9100), (Loopback::V6, 9100)]
        );
        assert!(parse_listening_sockets("").is_empty());
    }

    #[test]
    fn a_v6_only_server_is_addressed_as_v6() {
        let process = ServerProcess {
            pid: 1,
            csrf_token: "main".into(),
            extension_port: None,
            extension_csrf_token: None,
        };
        let endpoints = candidates(&process, &[(Loopback::V6, 9100)]);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].host, Loopback::V6);
        assert_eq!(endpoints[0].host.host(), "[::1]");
    }

    #[test]
    fn the_extension_server_is_tried_with_both_tokens() {
        let process = ServerProcess {
            pid: 1,
            csrf_token: "main".into(),
            extension_port: Some(5500),
            extension_csrf_token: Some("ext".into()),
        };
        let endpoints = candidates(&process, &[(Loopback::V4, 9100)]);
        let shapes: Vec<(&str, &str, u16, &str)> = endpoints
            .iter()
            .map(|e| (e.scheme, e.host.host(), e.port, e.csrf_token.as_str()))
            .collect();
        assert_eq!(
            shapes,
            vec![
                ("https", "127.0.0.1", 9100, "main"),
                ("http", "127.0.0.1", 5500, "ext"),
                ("http", "127.0.0.1", 5500, "main"),
                ("http", "[::1]", 5500, "ext"),
                ("http", "[::1]", 5500, "main"),
            ]
        );
        // With no separate extension token there is one HTTP shape per family.
        let process = ServerProcess {
            extension_csrf_token: None,
            ..process
        };
        assert_eq!(candidates(&process, &[]).len(), 2);
        // And with nothing to address at all, nothing is invented.
        let process = ServerProcess {
            extension_port: None,
            ..process
        };
        assert!(candidates(&process, &[]).is_empty());
    }
}
