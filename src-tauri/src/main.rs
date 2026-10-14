#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--mcp") {
        if let Err(e) = homemade_lib::integrations::mcp_server::run_stdio() {
            eprintln!("homemade --mcp: {e}");
            std::process::exit(1);
        }
        return;
    }
    // Any other argument = CLI mode (`homemade help`). No args (or a macOS launch-services `-psn_…` arg) = GUI.
    let gui = args.is_empty() || args.iter().all(|a| a.starts_with("-psn"));
    if !gui {
        std::process::exit(homemade_lib::control::cli::run(args));
    }
    homemade_lib::run()
}
