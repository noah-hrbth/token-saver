mod compressors;
mod init;
mod runner;
mod uninstall;

use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    let argv0 = &args[0];

    // Determine command name and command args.
    // If invoked as a symlink (argv[0] = "git"), command = "git", command_args = rest.
    // If invoked directly (argv[0] ends with "token-saver"), command = args[1], command_args = args[2..].
    let binary_name = PathBuf::from(argv0)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (command, command_args) = if binary_name == "token-saver" {
        match args.get(1).map(String::as_str) {
            Some("--version") | Some("-V") => {
                println!("token-saver {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            Some("--help") | Some("-h") => {
                println!("token-saver {}", env!("CARGO_PKG_VERSION"));
                println!(
                    "Transparent CLI proxy that compresses verbose command output for LLM agents."
                );
                println!();
                println!("USAGE:");
                println!(
                    "    token-saver <command> [args...]    Run command with compression (when TOKEN_SAVER=1)"
                );
                println!(
                    "    token-saver init                   Auto-configure shell profile + Claude Code settings.json"
                );
                println!(
                    "    token-saver init <shell>           Print shell-function block (zsh|bash)"
                );
                println!(
                    "    token-saver uninstall              Reverse `init` (clean shell profile + settings.json)"
                );
                println!("    token-saver --version              Print version");
                println!();
                println!("First-time setup (Homebrew or cargo install):");
                println!("    token-saver init                   # one-shot setup");
                return;
            }
            Some("init") => {
                let init_args: Vec<String> = args.iter().skip(2).cloned().collect();
                process::exit(init::run(&init_args));
            }
            Some("uninstall") => {
                let uninstall_args: Vec<String> = args.iter().skip(2).cloned().collect();
                process::exit(uninstall::run(&uninstall_args));
            }
            _ => {}
        }
        // Direct invocation: token-saver git status
        if args.len() < 2 {
            eprintln!("Usage: token-saver <command> [args...]");
            process::exit(1);
        }
        (args[1].clone(), args[2..].to_vec())
    } else {
        // Symlink invocation: argv[0] is the command name
        (binary_name, args[1..].to_vec())
    };

    // Determine our own binary's directory to skip in PATH lookups
    let self_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();

    // Find the real binary
    let real_binary = match runner::find_real_binary(&command, &self_dir) {
        Some(path) => path,
        None => {
            eprintln!("token-saver: {}: command not found", command);
            process::exit(127);
        }
    };

    // If TOKEN_SAVER is not set, passthrough directly
    let token_saver_enabled = env::var("TOKEN_SAVER").unwrap_or_default() == "1";
    if !token_saver_enabled {
        // exec replaces this process — does not return
        if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
            eprintln!("token-saver: failed to exec {}: {}", command, e);
            process::exit(1);
        }
        unreachable!();
    }

    // Try to find a compressor
    let compressor = compressors::find_compressor(&command, &command_args);

    match compressor {
        None => {
            // No compressor — passthrough
            if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                eprintln!("token-saver: failed to exec {}: {}", command, e);
                process::exit(1);
            }
        }
        Some(comp) => {
            // Run with normalized args, try to compress
            let normalized = comp.normalized_args(&command_args);
            match runner::execute_captured(&real_binary, &normalized) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let exit_code = output.status.code().unwrap_or(1);

                    match comp.compress(&stdout, &stderr, exit_code) {
                        Some(compressed) => {
                            print!("{}", compressed);
                            process::exit(exit_code);
                        }
                        None => {
                            // Compression failed — fall back to running with original args
                            if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                                eprintln!("token-saver: failed to exec {}: {}", command, e);
                                process::exit(1);
                            }
                        }
                    }
                }
                Err(_) => {
                    // Execution failed — fall back to passthrough
                    if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                        eprintln!("token-saver: failed to exec {}: {}", command, e);
                        process::exit(1);
                    }
                }
            }
        }
    }
}
