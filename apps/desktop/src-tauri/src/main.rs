// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--mcp-stdio")
    {
        std::process::exit(vibe_bar_desktop_lib::run_mcp_stdio());
    }
    vibe_bar_desktop_lib::run()
}
