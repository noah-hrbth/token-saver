use std::fs;
use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

/// Returns true if the tsc compressor scenarios can run.
/// Accepts either a globally installed `tsc` or a locally accessible `npm`
/// (setup functions install typescript locally via npm install).
pub fn is_available() -> bool {
    // Fast path: global tsc already in PATH
    if Command::new("tsc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }
    // Fallback: npm is available → setup will install typescript locally
    Command::new("npm")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Install typescript into the repo's local node_modules.
/// Called by every setup function so the binary is available without global install.
fn install_typescript(repo: &Path) {
    Command::new("npm")
        .args(["install", "--no-fund", "--no-audit", "--no-progress"])
        .current_dir(repo)
        .output()
        .expect("npm install failed");
}

/// All tsc scenarios. Requires tsc installed globally or via npm.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "TSC clean",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_clean,
            assertions: vec![
                Assertion::NotContains("error"),
                Assertion::NotContains("TS"),
            ],
        },
        Scenario {
            name: "TSC single-file errors",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_single_file_errors,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "TSC multi-file errors",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_multi_file_errors,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "TSC many errors across files",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_many_errors,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "TSC dedup heavy — 8 identical errors in one file",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_dedup_heavy,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "TSC chain errors — interface mismatch with continuations",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_chain_errors,
            assertions: vec![
                Assertion::Contains("TS2322"),
                Assertion::Contains("src/"),
                Assertion::Contains("incompatible"),
            ],
        },
        Scenario {
            name: "TSC repeated pattern — 4 files × 3 identical errors",
            command: "tsc",
            args: &["--noEmit"],
            setup: setup_repeated_pattern,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
    ]
}

/// Same scenarios routed through `npx tsc`.
pub fn npx_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "npx tsc clean",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_clean,
            assertions: vec![
                Assertion::NotContains("error"),
                Assertion::NotContains("TS"),
            ],
        },
        Scenario {
            name: "npx tsc single-file errors",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_single_file_errors,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "npx tsc multi-file errors",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_multi_file_errors,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "npx tsc many errors across files",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_many_errors,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "npx tsc dedup heavy — 8 identical errors in one file",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_dedup_heavy,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
        Scenario {
            name: "npx tsc chain errors — interface mismatch with continuations",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_chain_errors,
            assertions: vec![
                Assertion::Contains("TS2322"),
                Assertion::Contains("src/"),
                Assertion::Contains("incompatible"),
            ],
        },
        Scenario {
            name: "npx tsc repeated pattern — 4 files × 3 identical errors",
            command: "npx",
            args: &["tsc", "--noEmit"],
            setup: setup_repeated_pattern,
            assertions: vec![Assertion::Contains("TS2322"), Assertion::Contains("src/")],
        },
    ]
}

fn write_tsconfig(repo: &Path) {
    fs::write(
        repo.join("tsconfig.json"),
        r#"{ "compilerOptions": { "strict": true, "noEmit": true } }"#,
    )
    .unwrap();
}

fn write_package_json(repo: &Path) {
    fs::write(
        repo.join("package.json"),
        r#"{
  "name": "test-project",
  "private": true,
  "devDependencies": {
    "typescript": "^5.0.0"
  }
}"#,
    )
    .unwrap();
}

fn setup_clean(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/index.ts"),
        "const x: number = 1;\nexport { x };\n",
    )
    .unwrap();
}

fn setup_single_file_errors(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    // Deliberate type error: string assigned to number
    fs::write(
        repo.join("src/index.ts"),
        "const x: number = \"hello\";\nexport { x };\n",
    )
    .unwrap();
}

fn setup_multi_file_errors(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/a.ts"),
        "const x: number = \"hello\";\nexport { x };\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/b.ts"),
        "const y: number = \"world\";\nexport { y };\n",
    )
    .unwrap();
}

/// 5 files × 3 errors each — enough volume to show path-deduplication savings.
fn setup_many_errors(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src/api")).unwrap();
    fs::create_dir_all(repo.join("src/utils")).unwrap();

    // Each file has multiple type errors on different lines
    let files: &[(&str, &str)] = &[
        (
            "src/api/users.ts",
            concat!(
                "const id: number = \"abc\";\n",
                "const name: number = true;\n",
                "const email: number = null;\n",
                "export { id, name, email };\n",
            ),
        ),
        (
            "src/api/posts.ts",
            concat!(
                "const title: number = \"hello\";\n",
                "const body: number = [];\n",
                "const published: number = {};\n",
                "export { title, body, published };\n",
            ),
        ),
        (
            "src/utils/math.ts",
            concat!(
                "const sum: string = 42;\n",
                "const product: string = 3.14;\n",
                "const diff: boolean = 0;\n",
                "export { sum, product, diff };\n",
            ),
        ),
        (
            "src/utils/format.ts",
            concat!(
                "const pad: number = true;\n",
                "const trim: number = undefined;\n",
                "const upper: number = Symbol();\n",
                "export { pad, trim, upper };\n",
            ),
        ),
        (
            "src/index.ts",
            concat!(
                "const a: number = \"x\";\n",
                "const b: number = \"y\";\n",
                "const c: number = \"z\";\n",
                "export { a, b, c };\n",
            ),
        ),
    ];

    for (path, content) in files {
        fs::write(repo.join(path), content).unwrap();
    }
}

/// One file with 8 identical string→number type errors.
/// All generate `TS2322: Type 'string' is not assignable to type 'number'` — maximum dedup savings.
fn setup_dedup_heavy(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/component.ts"),
        concat!(
            "const a: number = \"x\";\n",
            "const b: number = \"x\";\n",
            "const c: number = \"x\";\n",
            "const d: number = \"x\";\n",
            "const e: number = \"x\";\n",
            "const f: number = \"x\";\n",
            "const g: number = \"x\";\n",
            "const h: number = \"x\";\n",
            "export { a, b, c, d, e, f, g, h };\n",
        ),
    )
    .unwrap();
}

/// Interface mismatch via variable assignment — produces "Types of property 'x' are
/// incompatible" chain continuations. Assigning through a typed variable (not a literal)
/// forces tsc to show the top-level mismatch + per-property chain lines.
/// Both assignments share identical primary + chain → dedup collapses them to one entry.
fn setup_chain_errors(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/index.ts"),
        concat!(
            "interface Foo { x: number; }\n",
            "const src1: { x: string } = { x: \"a\" };\n",
            "const src2: { x: string } = { x: \"b\" };\n",
            "const a: Foo = src1;\n",
            "const b: Foo = src2;\n",
            "export { a, b };\n",
        ),
    )
    .unwrap();
}

/// 4 files each with 3 identical string→number errors.
/// Within-file dedup collapses each file to one line; code-header fires per file.
/// Illustrates the remaining gap where cross-file dedup would eliminate repeated messages.
fn setup_repeated_pattern(repo: &Path) {
    write_tsconfig(repo);
    write_package_json(repo);
    install_typescript(repo);
    fs::create_dir_all(repo.join("src/components")).unwrap();

    let content = concat!(
        "const x: number = \"str\";\n",
        "const y: number = \"str\";\n",
        "const z: number = \"str\";\n",
        "export { x, y, z };\n",
    );
    for name in &["button", "input", "modal", "table"] {
        fs::write(repo.join(format!("src/components/{}.ts", name)), content).unwrap();
    }
}
