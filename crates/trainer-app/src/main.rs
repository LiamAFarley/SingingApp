// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--mcp") {
        // MCP over stdio for the LLM agent: no window, same data directory as the app.
        let data_dir = args
            .windows(2)
            .find(|w| w[0] == "--data-dir")
            .map(|w| std::path::PathBuf::from(&w[1]))
            .unwrap_or_else(trainer_core::storage::default_data_dir);
        if let Err(e) = trainer_mcp::run_stdio_blocking(data_dir) {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
        return;
    }
    trainer_app::run();
}
