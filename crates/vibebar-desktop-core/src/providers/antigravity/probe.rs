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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: &'static str,
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
        let url = format!("{}://127.0.0.1:{}{}", endpoint.scheme, endpoint.port, path);
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
    // `-ww` twice: Darwin's ps truncates the command to the terminal width
    // otherwise, and the AntiGravity path alone can run past it, hiding the
    // `--csrf_token` that follows.
    let output = run("/bin/ps", &["-axww", "-o", "pid=,command="], timeout)
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

async fn listening_ports(pid: i32, timeout: &Duration) -> Result<Vec<u16>, QuotaError> {
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
    let ports = parse_listening_ports(&output);
    if ports.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Antigravity is running but no listening ports found yet — wait a few seconds and retry".into(),
        ));
    }
    Ok(ports)
}

/// The ports out of `lsof -nP -iTCP -sTCP:LISTEN`, sorted and deduplicated.
pub fn parse_listening_ports(output: &str) -> Vec<u16> {
    let mut ports: Vec<u16> = output
        .lines()
        .filter(|line| line.contains("(LISTEN)"))
        .filter_map(|line| {
            let before = line.split("(LISTEN)").next()?.trim_end();
            let port = before.rsplit(':').next()?;
            port.parse().ok()
        })
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// Every shape worth trying for one server: each listening port over HTTPS,
/// then the extension server's plain-HTTP port with its own token and with
/// the main one, since builds differ on which it accepts.
pub fn candidates(process: &ServerProcess, ports: &[u16]) -> Vec<Endpoint> {
    let mut endpoints: Vec<Endpoint> = ports
        .iter()
        .map(|port| Endpoint {
            scheme: "https",
            port: *port,
            csrf_token: process.csrf_token.clone(),
        })
        .collect();
    if let Some(port) = process.extension_port {
        for token in [
            process.extension_csrf_token.as_ref(),
            Some(&process.csrf_token),
        ]
        .into_iter()
        .flatten()
        {
            let candidate = Endpoint {
                scheme: "http",
                port,
                csrf_token: token.clone(),
            };
            if !endpoints.contains(&candidate) {
                endpoints.push(candidate);
            }
        }
    }
    endpoints
}

/// Run a read-only helper and return its stdout, bounded in time and size.
async fn run(binary: &str, args: &[&str], timeout: &Duration) -> Result<String, String> {
    const MAX_OUTPUT: usize = 4 * 1024 * 1024;
    let mut child = tokio::process::Command::new(binary)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let collected = tokio::time::timeout(*timeout, async {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(stdout).take(MAX_OUTPUT as u64);
        let mut buffer = Vec::new();
        let read = reader.read_to_end(&mut buffer).await;
        let status = child.wait().await;
        (read, status, buffer)
    })
    .await;
    match collected {
        Ok((Ok(_), _, buffer)) => Ok(String::from_utf8_lossy(&buffer).into_owned()),
        Ok((Err(error), _, _)) => Err(error.to_string()),
        Err(_) => Err("timed out".into()),
    }
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
    fn listening_ports_come_out_sorted_and_deduplicated() {
        let output = concat!(
            "COMMAND   PID USER   FD  TYPE            DEVICE SIZE/OFF NODE NAME\n",
            "language_ 501 me    30u  IPv4 0x1              0t0  TCP 127.0.0.1:9100 (LISTEN)\n",
            "language_ 501 me    31u  IPv6 0x2              0t0  TCP [::1]:9100 (LISTEN)\n",
            "language_ 501 me    32u  IPv4 0x3              0t0  TCP 127.0.0.1:8080 (LISTEN)\n",
            "language_ 501 me    33u  IPv4 0x4              0t0  TCP 127.0.0.1:7000->1.2.3.4:443 (ESTABLISHED)\n",
        );
        assert_eq!(parse_listening_ports(output), vec![8080, 9100]);
        assert!(parse_listening_ports("").is_empty());
    }

    #[test]
    fn the_extension_server_is_tried_with_both_tokens() {
        let process = ServerProcess {
            pid: 1,
            csrf_token: "main".into(),
            extension_port: Some(5500),
            extension_csrf_token: Some("ext".into()),
        };
        let endpoints = candidates(&process, &[9100]);
        assert_eq!(
            endpoints,
            vec![
                Endpoint {
                    scheme: "https",
                    port: 9100,
                    csrf_token: "main".into()
                },
                Endpoint {
                    scheme: "http",
                    port: 5500,
                    csrf_token: "ext".into()
                },
                Endpoint {
                    scheme: "http",
                    port: 5500,
                    csrf_token: "main".into()
                },
            ]
        );
        // With no separate extension token there is one HTTP shape, not two.
        let process = ServerProcess {
            extension_csrf_token: None,
            ..process
        };
        assert_eq!(candidates(&process, &[]).len(), 1);
        // And with nothing to address at all, nothing is invented.
        let process = ServerProcess {
            extension_port: None,
            ..process
        };
        assert!(candidates(&process, &[]).is_empty());
    }
}
