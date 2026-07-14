#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // CLI mode: skip the Tauri window when flags are present
    if args.len() > 1 {
        if let Err(err) = kaiser_app_lib::cli::run(&args[1..]) {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
        return;
    }

    kaiser_app_lib::run();
}
