use std::fs;
use std::path::Path;

use super::{Assertion, Scenario};

/// All grep/rg scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Recursive grep multifile grouping",
            command: "grep",
            args: &["-rn", "fn", "."],
            setup: setup_deep_multifile,
            assertions: vec![
                Assertion::Contains("src/main.rs"),
                Assertion::Contains("src/handlers/api.rs"),
                Assertion::Contains("fn main"),
                // Compressed format puts filename on its own line; raw format has file:line:content.
                // Verify the raw colon-separated format is gone.
                Assertion::NotContains("src/main.rs:1:"),
            ],
        },
        Scenario {
            name: "Grep with context",
            command: "grep",
            args: &["-rn", "-A", "1", "println", "."],
            setup: setup_context,
            assertions: vec![
                Assertion::Contains("println"),
                // Two non-adjacent match groups produce a "  --" separator between them.
                Assertion::Contains("  --"),
            ],
        },
        Scenario {
            name: "Grep single file no grouping",
            command: "grep",
            args: &["-n", "line", "data.txt"],
            setup: setup_single_file,
            assertions: vec![
                Assertion::Contains("line one"),
                Assertion::Contains("line two"),
                // Single-file mode passes output through as-is; no file-header line.
                Assertion::NotContains("data.txt\n"),
            ],
        },
        Scenario {
            name: "Grep many matches cap",
            command: "grep",
            args: &["-n", "match", "big.txt"],
            setup: setup_big_file,
            assertions: vec![
                Assertion::Contains("... and 10 more matches"),
                Assertion::Contains("match line 1"),
            ],
        },
        Scenario {
            name: "rg recursive multifile grouping",
            command: "rg",
            args: &["-n", "fn", "."],
            setup: setup_deep_multifile,
            assertions: vec![
                Assertion::Contains("fn main"),
                Assertion::Contains("fn handle_get"),
                Assertion::Contains("fn process"),
                Assertion::Contains("src/handlers/api.rs"),
            ],
        },
    ]
}

fn setup_context(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    // Two println calls separated by 3 lines so their -C1 context windows don't overlap,
    // causing grep to emit a "--" separator between the two groups.
    fs::write(
        repo.join("src/main.rs"),
        "fn main() {\n    println!(\"first\");\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    println!(\"second\");\n    let d = 4;\n}\n",
    )
    .unwrap();
}

fn setup_single_file(repo: &Path) {
    fs::write(
        repo.join("data.txt"),
        "line one\nfoo bar\nline two\nbaz\nline three\n",
    )
    .unwrap();
}

fn setup_deep_multifile(repo: &Path) {
    fs::create_dir_all(repo.join("src/handlers")).unwrap();
    fs::create_dir_all(repo.join("src/models")).unwrap();
    fs::create_dir_all(repo.join("src/utils")).unwrap();
    fs::create_dir_all(repo.join("tests/integration")).unwrap();

    fs::write(
        repo.join("src/main.rs"),
        "fn main() {\n    let app = setup();\n    run(app);\n}\n\nfn setup() -> App {\n    App::new()\n}\n\nfn run(app: App) {\n    app.start();\n}\n",
    ).unwrap();
    fs::write(
        repo.join("src/handlers/api.rs"),
        "pub fn handle_get(req: Request) -> Response {\n    let data = fetch_data();\n    Response::ok(data)\n}\n\npub fn handle_post(req: Request) -> Response {\n    let body = req.body();\n    process(body)\n}\n\nfn fetch_data() -> Data {\n    Data::default()\n}\n\nfn process(body: Body) -> Response {\n    Response::created()\n}\n",
    ).unwrap();
    fs::write(
        repo.join("src/handlers/auth.rs"),
        "pub fn login(creds: Credentials) -> Token {\n    validate(creds);\n    Token::generate()\n}\n\npub fn logout(token: Token) {\n    token.revoke();\n}\n\nfn validate(creds: Credentials) -> bool {\n    creds.verify()\n}\n",
    ).unwrap();
    fs::write(
        repo.join("src/models/user.rs"),
        "pub fn new(name: &str) -> User {\n    User { name: name.to_string() }\n}\n\npub fn find_by_id(id: u64) -> Option<User> {\n    db::query(id)\n}\n\nfn from_row(row: Row) -> User {\n    User { name: row.get(\"name\") }\n}\n",
    ).unwrap();
    fs::write(
        repo.join("src/utils/helpers.rs"),
        "pub fn format_date(ts: i64) -> String {\n    chrono::format(ts)\n}\n\npub fn hash_password(pw: &str) -> String {\n    bcrypt::hash(pw)\n}\n\nfn sanitize(input: &str) -> String {\n    input.trim().to_string()\n}\n",
    ).unwrap();
    fs::write(
        repo.join("tests/integration/api_test.rs"),
        "fn test_get_returns_200() {\n    let resp = client.get(\"/api\");\n    assert_eq!(resp.status(), 200);\n}\n\nfn test_post_creates_resource() {\n    let resp = client.post(\"/api\", body);\n    assert_eq!(resp.status(), 201);\n}\n\nfn test_auth_required() {\n    let resp = client.get(\"/api/protected\");\n    assert_eq!(resp.status(), 401);\n}\n",
    ).unwrap();
}

fn setup_big_file(repo: &Path) {
    let content: String = (1..=210).map(|i| format!("match line {}\n", i)).collect();
    fs::write(repo.join("big.txt"), content).unwrap();
}
