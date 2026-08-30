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
    let root = match std::env::args().nth(1) {
        Some(path) => DataRoot::at(path),
        None => DataRoot::discover(),
    };
    println!("data root: {}", root.shared().display());
    println!("demo mode: {}", root.is_demo());

    let settings = SharedSettings::load(&root);
    let (fields, labels) = settings.menu_bar_fields();
    println!(
        "\nshared settings: refresh {}s, shows {}, {} menu-bar fields, {} custom labels",
        settings.refresh_interval().as_secs(),
        if settings.shows_remaining() { "remaining" } else { "used" },
        fields.len(),
        labels.len()
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

    let status = service_status::load(&root);
    let degraded: Vec<&str> = status
        .iter()
        .filter(|(_, snapshot)| snapshot.is_degraded())
        .map(|(tool, _)| tool.raw_value())
        .collect();
    println!(
        "\nservice status: {} providers cached, degraded: {}",
        status.len(),
        if degraded.is_empty() { "none".to_string() } else { degraded.join(", ") }
    );

    let scan_home = if root.is_demo() {
        root.shared().parent().unwrap_or(root.shared()).to_path_buf()
    } else {
        home_directory()
    };
    let cost = CostEngine::new(scan_home).refresh().unwrap_or_default();
    println!(
        "\nlocal usage: {} files, {} requests, {} tokens, {} unpriced, truncated={}",
        cost.scanned_files,
        cost.all_time.requests,
        cost.all_time.tokens,
        cost.unpriced_events,
        cost.truncated
    );

    let sessions = SessionsService::new(root);
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
            row.title.as_deref().unwrap_or("<untitled>").chars().take(60).collect::<String>()
        );
    }
    if listing.source == SessionSource::Indexed {
        let hits = sessions.search("quota", 3);
        println!("  search 'quota': {} hits", hits.rows.len());
    }
}
