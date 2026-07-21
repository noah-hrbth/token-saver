mod agents;
mod compressors;
mod install;
mod runner;
mod shell_hook;
mod uninstall;
mod wizard;

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process;

fn main() {
    // args_os, not args(): env::args() panics on a non-UTF-8 argument (e.g. a
    // filename with invalid bytes), which would crash the proxy before it could
    // even pass the command through. argv is kept as OsString so exact bytes
    // survive to the passthrough exec.
    let args: Vec<OsString> = env::args_os().collect();
    let argv0 = match args.first() {
        Some(a) => a,
        None => process::exit(1),
    };

    // Determine command name and command args.
    // If invoked as a symlink (argv[0] = "git"), command = "git", command_args = rest.
    // If invoked directly (argv[0] ends with "token-saver"), command = args[1], command_args = args[2..].
    let binary_name = PathBuf::from(argv0)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (command, command_args): (String, Vec<OsString>) = if binary_name == "token-saver" {
        match args.get(1).and_then(|a| a.to_str()) {
            Some("version") => {
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
                    "    token-saver install                Interactive setup wizard (shell profile + agent configs)"
                );
                println!(
                    "    token-saver install <shell>        Print shell-function block (zsh|bash)"
                );
                println!(
                    "    token-saver uninstall              Reverse `install` (shell profile + agent configs)"
                );
                println!("    token-saver version                Print version");
                println!();
                println!("First-time setup (Homebrew or cargo install):");
                println!("    token-saver install                # one-shot setup");
                return;
            }
            Some("install") | Some("init") => {
                let install_args: Vec<String> = args.iter().skip(2).map(arg_to_string).collect();
                process::exit(install::run(&install_args));
            }
            Some("uninstall") => {
                let uninstall_args: Vec<String> = args.iter().skip(2).map(arg_to_string).collect();
                process::exit(uninstall::run(&uninstall_args));
            }
            _ => {}
        }
        // Direct invocation: token-saver git status
        if args.len() < 2 {
            eprintln!("Usage: token-saver <command> [args...]");
            process::exit(1);
        }
        (arg_to_string(&args[1]), args[2..].to_vec())
    } else {
        // Symlink invocation: argv[0] is the command name
        (binary_name, args[1..].to_vec())
    };

    // Path to our own executable, so PATH lookup can skip it (and any wrapper
    // symlink pointing at it) without excluding real tools that happen to live
    // in the same directory — e.g. a brew-installed tool next to a
    // brew-installed token-saver.
    let self_exe = env::current_exe().unwrap_or_default();

    // Find the real binary
    let real_binary = match runner::find_real_binary(&command, &self_exe) {
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

    // Compressors operate on &str args; if any arg is not valid UTF-8 we cannot
    // compress and must pass the exact bytes through unchanged.
    let command_args_utf8: Option<Vec<String>> = command_args
        .iter()
        .map(|a| a.to_str().map(str::to_string))
        .collect();

    let compressor = command_args_utf8
        .as_ref()
        .and_then(|utf8| compressors::find_compressor(&command, utf8));

    match compressor {
        None => {
            // No compressor (or non-UTF-8 args) — passthrough
            if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                eprintln!("token-saver: failed to exec {}: {}", command, e);
                process::exit(1);
            }
        }
        Some(comp) => {
            // a compressor is only returned when command_args_utf8 is Some
            let utf8_args = command_args_utf8.expect("compressor implies utf8 args");
            let normalized = comp.normalized_args(&utf8_args);
            match runner::execute_captured(&real_binary, &normalized) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let exit_code = exit_code_of(&output.status);

                    match comp.compress(&stdout, &stderr, exit_code) {
                        Some(compressed) => {
                            print!("{}", compressed);
                            process::exit(exit_code);
                        }
                        None => {
                            // Compression declined. Re-running with the user's original
                            // args gives faithful output, but a side-effecting command
                            // (prettier --write, jest) already ran once via the capture —
                            // re-execing would double the effect/cost, so emit the
                            // captured output instead.
                            if comp.side_effects() {
                                print!("{}", stdout);
                                eprint!("{}", stderr);
                                process::exit(exit_code);
                            }
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

/// Convert an argv element to a String, replacing any non-UTF-8 bytes. Used only
/// for token-saver's own subcommands (install/uninstall), whose args are ASCII.
fn arg_to_string(arg: &OsString) -> String {
    arg.to_string_lossy().into_owned()
}

/// Exit code of a finished child, mapping signal-termination to the shell's
/// 128+signal convention rather than collapsing it to 1 (which a compressor
/// could misread as a clean result).
fn exit_code_of(status: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| status.signal().map(|s| 128 + s).unwrap_or(1))
}
