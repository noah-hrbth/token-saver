use crate::compressors::Compressor;
use crate::compressors::eslint;
use crate::compressors::jest;
use crate::compressors::prettier;

/// Boolean npx flags (no value follows).
const NPX_BOOL_FLAGS: &[&str] = &["--yes", "-y", "--no", "-n", "--quiet", "-q"];

/// npx flags that consume the next argument as a value.
const NPX_VALUE_FLAGS: &[&str] = &["--package", "-p", "--shell", "-s"];

/// npx flags that indicate a non-interceptable invocation (shell script execution).
const NPX_SKIP_FLAGS: &[&str] = &["--call", "-c"];

pub struct NpxCompressor {
    sub_compressor: Box<dyn Compressor>,
}

/// Parse npx args to extract the command name and its arguments.
///
/// Returns `Some((command, command_args))` when a command is found, `None` when no
/// command is present or when a skip flag (`--call`, `-c`) is encountered.
fn parse_npx_args(args: &[String]) -> Option<(String, Vec<String>)> {
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].as_str();

        // "--" terminates npx flags — everything after is command + args
        if arg == "--" {
            let rest = &args[i + 1..];
            return rest.first().map(|cmd| (cmd.clone(), rest[1..].to_vec()));
        }

        // --call/-c means shell script execution — can't intercept
        if NPX_SKIP_FLAGS.contains(&arg) || arg.starts_with("--call=") || arg.starts_with("-c=") {
            return None;
        }

        // Boolean flags: consume just this arg
        if NPX_BOOL_FLAGS.contains(&arg) {
            i += 1;
            continue;
        }

        // Value flags: consume this arg + next
        if NPX_VALUE_FLAGS.contains(&arg) {
            i += 2;
            continue;
        }
        // Handle --package=foo, -p=foo, --shell=zsh, -s=zsh
        if arg.starts_with("--package=")
            || arg.starts_with("-p=")
            || arg.starts_with("--shell=")
            || arg.starts_with("-s=")
        {
            i += 1;
            continue;
        }

        // First unrecognized positional = the command name
        return Some((arg.to_string(), args[i + 1..].to_vec()));
    }

    // Ran out of args without finding a command
    None
}

/// Reconstruct npx args with the command portion replaced by normalized command args.
fn rebuild_npx_args(
    original_args: &[String],
    command: &str,
    command_normalized: &[String],
) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < original_args.len() {
        let arg = original_args[i].as_str();

        if arg == "--" {
            result.push("--".to_string());
            result.push(command.to_string());
            result.extend_from_slice(command_normalized);
            return result;
        }

        if NPX_BOOL_FLAGS.contains(&arg) {
            result.push(original_args[i].clone());
            i += 1;
            continue;
        }

        if NPX_VALUE_FLAGS.contains(&arg) {
            result.push(original_args[i].clone());
            if i + 1 < original_args.len() {
                result.push(original_args[i + 1].clone());
            }
            i += 2;
            continue;
        }

        if arg.starts_with("--package=")
            || arg.starts_with("-p=")
            || arg.starts_with("--shell=")
            || arg.starts_with("-s=")
        {
            result.push(original_args[i].clone());
            i += 1;
            continue;
        }

        // This is the command name — replace with normalized
        result.push(command.to_string());
        result.extend_from_slice(command_normalized);
        return result;
    }

    // Fallback (shouldn't happen if parse_npx_args succeeded)
    result.push(command.to_string());
    result.extend_from_slice(command_normalized);
    result
}

impl Compressor for NpxCompressor {
    fn can_compress(&self, _args: &[String]) -> bool {
        true // already validated at construction in find_compressor
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let (command, cmd_args) = parse_npx_args(original_args)
            .expect("normalized_args called without successful parse_npx_args");
        let cmd_normalized = self.sub_compressor.normalized_args(&cmd_args);
        rebuild_npx_args(original_args, &command, &cmd_normalized)
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        self.sub_compressor.compress(stdout, stderr, exit_code)
    }
}

/// Returns a compressor for npx commands if the underlying command is supported.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let (cmd, cmd_args) = parse_npx_args(args)?;
    let sub: Box<dyn Compressor> = match cmd.as_str() {
        "eslint" => {
            if eslint::has_skip_flag(&cmd_args) {
                return None;
            }
            Box::new(eslint::EslintCompressor)
        }
        "jest" => jest::find_compressor(&cmd_args)?,
        "prettier" => prettier::find_compressor(&cmd_args)?,
        _ => return None,
    };
    Some(Box::new(NpxCompressor {
        sub_compressor: sub,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // --- parse_npx_args ---

    #[test]
    fn parse_bare_eslint() {
        let result = parse_npx_args(&args(&["eslint", "src/"]));
        assert_eq!(
            result,
            Some(("eslint".to_string(), vec!["src/".to_string()]))
        );
    }

    #[test]
    fn parse_eslint_with_bool_flags() {
        let result = parse_npx_args(&args(&["--yes", "--quiet", "eslint", "src/"]));
        assert_eq!(
            result,
            Some(("eslint".to_string(), vec!["src/".to_string()]))
        );
    }

    #[test]
    fn parse_eslint_with_value_flags() {
        let result = parse_npx_args(&args(&[
            "--package",
            "eslint@8",
            "eslint",
            "--no-eslintrc",
            ".",
        ]));
        assert_eq!(
            result,
            Some((
                "eslint".to_string(),
                vec!["--no-eslintrc".to_string(), ".".to_string()]
            ))
        );
    }

    #[test]
    fn parse_eslint_after_double_dash() {
        let result = parse_npx_args(&args(&["--yes", "--", "eslint", "src/"]));
        assert_eq!(
            result,
            Some(("eslint".to_string(), vec!["src/".to_string()]))
        );
    }

    #[test]
    fn parse_not_eslint() {
        let result = parse_npx_args(&args(&["prettier", "src/"]));
        assert_eq!(
            result,
            Some(("prettier".to_string(), vec!["src/".to_string()]))
        );
    }

    #[test]
    fn parse_call_flag_passthrough() {
        let result = parse_npx_args(&args(&["--call", "eslint src/"]));
        assert_eq!(result, None);
    }

    #[test]
    fn parse_c_flag_passthrough() {
        let result = parse_npx_args(&args(&["-c", "eslint src/"]));
        assert_eq!(result, None);
    }

    #[test]
    fn parse_call_equals_passthrough() {
        let result = parse_npx_args(&args(&["--call=eslint src/"]));
        assert_eq!(result, None);
    }

    #[test]
    fn parse_package_equals_form() {
        let result = parse_npx_args(&args(&["-p=eslint@9", "eslint", "src/"]));
        assert_eq!(
            result,
            Some(("eslint".to_string(), vec!["src/".to_string()]))
        );
    }

    #[test]
    fn parse_no_args() {
        let result = parse_npx_args(&args(&[]));
        assert_eq!(result, None);
    }

    #[test]
    fn parse_only_npx_flags() {
        let result = parse_npx_args(&args(&["--yes", "--quiet"]));
        assert_eq!(result, None);
    }

    #[test]
    fn parse_eslint_no_extra_args() {
        let result = parse_npx_args(&args(&["eslint"]));
        assert_eq!(result, Some(("eslint".to_string(), vec![])));
    }

    // --- find_compressor ---

    #[test]
    fn find_compressor_eslint() {
        let result = find_compressor(&args(&["eslint", "src/"]));
        assert!(result.is_some());
    }

    #[test]
    fn find_compressor_unknown_command() {
        let result = find_compressor(&args(&["tsc", "src/"]));
        assert!(result.is_none());
    }

    #[test]
    fn find_compressor_eslint_with_fix() {
        let result = find_compressor(&args(&["eslint", "--fix", "src/"]));
        assert!(result.is_none());
    }

    #[test]
    fn find_compressor_eslint_with_format() {
        let result = find_compressor(&args(&["eslint", "--format", "stylish", "src/"]));
        assert!(result.is_none());
    }

    // --- normalized_args (eslint) ---

    #[test]
    fn normalized_args_bare_eslint() {
        let input = args(&["eslint", "src/"]);
        let c = find_compressor(&input).unwrap();
        assert_eq!(
            c.normalized_args(&input),
            args(&["eslint", "src/", "--format", "json"])
        );
    }

    #[test]
    fn normalized_args_preserves_npx_flags() {
        let input = args(&["--yes", "--package", "eslint@9", "eslint", "src/"]);
        let c = find_compressor(&input).unwrap();
        assert_eq!(
            c.normalized_args(&input),
            args(&[
                "--yes",
                "--package",
                "eslint@9",
                "eslint",
                "src/",
                "--format",
                "json"
            ])
        );
    }

    #[test]
    fn normalized_args_with_double_dash() {
        let input = args(&["--yes", "--", "eslint", "src/"]);
        let c = find_compressor(&input).unwrap();
        assert_eq!(
            c.normalized_args(&input),
            args(&["--yes", "--", "eslint", "src/", "--format", "json"])
        );
    }

    // --- prettier via npx ---

    #[test]
    fn find_compressor_prettier_check() {
        assert!(find_compressor(&args(&["prettier", "--check", "src/"])).is_some());
    }

    #[test]
    fn find_compressor_prettier_write() {
        assert!(find_compressor(&args(&["prettier", "--write", "src/"])).is_some());
    }

    #[test]
    fn find_compressor_prettier_bare() {
        assert!(find_compressor(&args(&["prettier", "src/"])).is_none());
    }

    #[test]
    fn find_compressor_prettier_skip_help() {
        assert!(find_compressor(&args(&["prettier", "--help"])).is_none());
    }

    #[test]
    fn find_compressor_prettier_skip_list_different() {
        assert!(find_compressor(&args(&["prettier", "-l", "src/"])).is_none());
    }

    #[test]
    fn normalized_args_prettier_check() {
        let input = args(&["prettier", "--check", "src/"]);
        let c = find_compressor(&input).unwrap();
        assert_eq!(
            c.normalized_args(&input),
            args(&["prettier", "--check", "src/", "--no-color"])
        );
    }
}
