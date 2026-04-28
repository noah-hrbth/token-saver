const COMMANDS: &[&str] = &[
    "cat", "eslint", "git", "jest", "ls", "find", "grep", "npx", "prettier", "rg", "tsc",
];

pub fn print(shell: &str) -> i32 {
    match shell {
        "zsh" | "bash" => {
            println!("# token-saver: wrap commands for LLM output compression");
            println!("# Loads only when TOKEN_SAVER=1 — no-op otherwise.");
            println!("if [ \"$TOKEN_SAVER\" = \"1\" ]; then");
            for cmd in COMMANDS {
                println!("    {cmd}() {{ command token-saver {cmd} \"$@\"; }}");
            }
            println!("fi");
            0
        }
        "" => {
            eprintln!("token-saver init: missing shell argument (zsh|bash)");
            2
        }
        other => {
            eprintln!("token-saver init: unsupported shell '{other}' (supported: zsh, bash)");
            2
        }
    }
}
