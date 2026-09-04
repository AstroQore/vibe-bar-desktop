//! Read-only inspection of a real Vibe Bar data root, used to verify that
//! Desktop reads the shared stores correctly and writes nothing into them.
//!
//! `cargo run --example inspect` (add `--` `<root>` to point elsewhere).
//! Prints no credentials, tokens, emails, or account identifiers.

use vibebar_desktop_core::cost::CostEngine;
use vibebar_desktop_core::paths::{home_directory, DataRoot};
use vibebar_desktop_core::refresh::QuotaEngine;
use vibebar_desktop_core::sessions::{SessionSource, SessionsService};
use vibebar_desktop_core::shared::{service_status, settings::SharedSettings};

fn main() {
    // An explicit argument is either a synthetic demo home or a real data
    // root on a platform whose default this example cannot guess. Which one
    // it is decides where sessions and usage are scanned from, and
    // `DataRoot::at` cannot answer that: its demo flag means "write nothing",
    // not "this path is synthetic".
    let explicit_root = std::env::args().nth(1);
    let root = match &explicit_root {
        Some(path) => DataRoot::at(path),
        None => DataRoot::discover(),
    };
    println!("data root: {}", root.shared().display());
    println!("demo mode: {}", root.is_demo());

    let settings = SharedSettings::load(&root);
    println!(
        "\nshared settings: refresh {}s, shows {}, mini {} ({})",
        settings.refresh_interval().as_secs(),
        if settings.shows_remaining() {
            "remaining"
        } else {
            "used"
        },
        settings.mini_display_mode(),
        settings.mini_strip_density()
    );

    let engine = QuotaEngine::new(root.clone());
    let view = engine.cached_view();
    println!(
        "\nquota: {} accounts, shared data present: {}",
        view.accounts.len(),
        view.has_shared_data
    );
    for account in view.accounts.iter().take(40) {
        let hierarchy = account.tool.hierarchy();
        let buckets: Vec<String> = account
            .buckets
            .iter()
            .map(|b| format!("{}={:.0}%", b.id, b.remaining_percent()))
            .collect();
        println!(
            "  {} / {} [{:?}] {}",
            hierarchy.vendor,
            hierarchy.product,
            account.origin,
            if buckets.is_empty() {
                "(no buckets cached)".to_string()
            } else {
                buckets.join(" ")
            }
        );
    }

    // What the quota bars cannot say on their own.
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let adopted = vibebar_desktop_core::forecast::seed_from_native_once(&root, now);
        let mut view = vibebar_desktop_core::shared::quota_cache::load_all(
            &root,
            &vibebar_desktop_core::shared::settings::SharedSettings::load(&root)
                .candidate_account_ids(),
        );
        vibebar_desktop_core::forecast::attach_forecasts(&root, &mut view, now);
        let forecast_count = view
            .iter()
            .flat_map(|a| a.buckets.iter())
            .filter(|b| b.forecast.is_some())
            .count();
        println!("\nforecast: {forecast_count} buckets, {adopted} observations adopted");
        for account in view.iter() {
            for bucket in account.buckets.iter().filter(|b| b.forecast.is_some()) {
                let f = bucket.forecast.as_ref().expect("checked");
                println!(
                    "  {}/{}: {:?} ({:?}) projected {:.0}% at reset",
                    account.tool.raw_value(),
                    bucket.id,
                    f.verdict,
                    f.confidence,
                    f.projected_used_percent
                );
            }
        }
    }

    let status = service_status::load(&root);
    let degraded: Vec<&str> = status
        .iter()
        .filter(|(_, snapshot)| snapshot.is_degraded())
        .map(|(tool, _)| tool.raw_value())
        .collect();
    println!(
        "\nservice status: {} providers cached, degraded: {}",
        status.len(),
        if degraded.is_empty() {
            "none".to_string()
        } else {
            degraded.join(", ")
        }
    );

    // A synthetic home keeps its data root's parent as the scan root, so a
    // demo tree stays self-contained. Anything else -- including an explicit
    // real root such as %APPDATA%\\VibeBar -- scans the user's actual home,
    // because %APPDATA%\\.codex does not exist and reporting zero sessions
    // there would be a lie rather than a finding.
    let scan_home = match explicit_root.as_deref().and_then(synthetic_home_of) {
        Some(home) => home,
        None => home_directory(),
    };
    // Keep this diagnostic read-only even on a real root. Re-wrapping the
    // exact path as demo suppresses only Desktop snapshot persistence.
    let cost = CostEngine::new(DataRoot::at(root.shared()), scan_home.clone())
        .refresh()
        .unwrap_or_default();
    println!(
        "\nlocal usage: {} files, {} requests, {} tokens, {} unpriced, truncated={}",
        cost.scanned_files,
        cost.all_time.requests,
        cost.all_time.tokens,
        cost.unpriced_events,
        cost.truncated
    );

    // Same scan root as the cost engine above: with a demo root this must
    // never fall back to the real home's session logs.
    let sessions = SessionsService::with_home(root, scan_home);
    let listing = sessions.list(5);
    println!(
        "\nsessions: source={:?}{}",
        listing.source,
        listing
            .indexed_total
            .map(|n| format!(", {n} indexed"))
            .unwrap_or_default()
    );
    if let Some(note) = &listing.index_note {
        println!("  note: {note}");
    }
    for row in &listing.rows {
        println!(
            "  [{}] {}",
            row.harness,
            row.title
                .as_deref()
                .unwrap_or("<untitled>")
                .chars()
                .take(60)
                .collect::<String>()
        );
    }
    if listing.source == SessionSource::Indexed {
        let hits = sessions.search("quota", 3);
        println!("  search 'quota': {} hits", hits.rows.len());
    }
}

/// The scan root for a synthetic data root, or `None` when the path looks
/// like a real one.
///
/// A demo tree is laid out as `<synthetic-home>/.vibebar`, so the home is the
/// parent — but only when the leaf is actually `.vibebar`. A platform data
/// root such as `%APPDATA%\\VibeBar` or `~/Library/Application Support/...`
/// has a different leaf and a parent that holds no agent logs.
fn synthetic_home_of(root: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(root);
    if path.file_name()? != std::ffi::OsStr::new(".vibebar") {
        return None;
    }
    Some(path.parent()?.to_path_buf())
}
