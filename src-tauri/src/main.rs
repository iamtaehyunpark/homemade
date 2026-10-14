#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if std::env::args().any(|a| a == "--mcp") {
        if let Err(e) = homemade_lib::integrations::mcp_server::run_stdio() {
            eprintln!("homemade --mcp: {e}");
            std::process::exit(1);
        }
        return;
    }
    homemade_lib::run()
}
