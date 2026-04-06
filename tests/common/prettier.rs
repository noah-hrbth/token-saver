use std::fs;
use std::path::Path;

use super::{Assertion, Scenario};

/// All prettier scenarios. Requires prettier installed globally or via npx.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Prettier --check single file",
            command: "prettier",
            args: &["--check", "src/"],
            setup: setup_unformatted,
            assertions: vec![
                Assertion::Contains("  ugly.js"),
                Assertion::Contains("needs formatting"),
                Assertion::NotContains("Checking formatting"),
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("Code style issues"),
            ],
        },
        Scenario {
            name: "Prettier --check many files",
            command: "prettier",
            args: &["--check", "src/"],
            setup: setup_many_unformatted,
            assertions: vec![
                Assertion::Contains("files need formatting"),
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("Checking formatting"),
                Assertion::NotContains("Code style issues"),
            ],
        },
        Scenario {
            name: "Prettier --check nested dirs",
            command: "prettier",
            args: &["--check", "src/"],
            setup: setup_nested_unformatted,
            assertions: vec![
                Assertion::Contains("src/components/"),
                Assertion::Contains("src/utils/"),
                Assertion::Contains("files need formatting"),
                Assertion::NotContains("[warn]"),
            ],
        },
        Scenario {
            name: "Prettier --check clean project",
            command: "prettier",
            args: &["--check", "src/clean.js"],
            setup: setup_clean,
            assertions: vec![
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("needs formatting"),
            ],
        },
        Scenario {
            name: "Prettier --write many files",
            command: "prettier",
            args: &["--write", "src/"],
            setup: setup_many_unformatted,
            assertions: vec![
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("Checking formatting"),
            ],
        },
    ]
}

/// Same scenarios routed through `npx prettier` instead of bare `prettier`.
pub fn npx_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "npx prettier --check single file",
            command: "npx",
            args: &["prettier", "--check", "src/"],
            setup: setup_unformatted,
            assertions: vec![
                Assertion::Contains("  ugly.js"),
                Assertion::Contains("needs formatting"),
                Assertion::NotContains("Checking formatting"),
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("Code style issues"),
            ],
        },
        Scenario {
            name: "npx prettier --check many files",
            command: "npx",
            args: &["prettier", "--check", "src/"],
            setup: setup_many_unformatted,
            assertions: vec![
                Assertion::Contains("files need formatting"),
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("Checking formatting"),
                Assertion::NotContains("Code style issues"),
            ],
        },
        Scenario {
            name: "npx prettier --check nested dirs",
            command: "npx",
            args: &["prettier", "--check", "src/"],
            setup: setup_nested_unformatted,
            assertions: vec![
                Assertion::Contains("src/components/"),
                Assertion::Contains("src/utils/"),
                Assertion::Contains("files need formatting"),
                Assertion::NotContains("[warn]"),
            ],
        },
        Scenario {
            name: "npx prettier --check clean project",
            command: "npx",
            args: &["prettier", "--check", "src/clean.js"],
            setup: setup_clean,
            assertions: vec![
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("needs formatting"),
            ],
        },
        Scenario {
            name: "npx prettier --write many files",
            command: "npx",
            args: &["prettier", "--write", "src/"],
            setup: setup_many_unformatted,
            assertions: vec![
                Assertion::NotContains("[warn]"),
                Assertion::NotContains("Checking formatting"),
            ],
        },
    ]
}

/// Unformatted JS content with obvious formatting violations.
const UGLY_JS: &str = "const   x   =   1\nconst y   =    'hello'\nif(true){console.log(x,y)}\nfunction   foo(  a,b,c  ){return    a+b+c}\nconst   arr=[1,2,3,4,5].map(  (x)  =>  x*2  )\n";

fn setup_unformatted(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/ugly.js"), UGLY_JS).unwrap();
}

fn setup_clean(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/clean.js"), "const x = 1;\nconsole.log(x);\n").unwrap();
}

fn setup_many_unformatted(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();

    let files = [
        "app.js",
        "config.js",
        "database.js",
        "helpers.js",
        "index.js",
        "logger.js",
        "middleware.js",
        "router.js",
        "server.js",
        "utils.js",
    ];

    for (i, name) in files.iter().enumerate() {
        let content = format!(
            "const   val{}   =   {}\nfunction   fn{}(  a,b  ){{return    a+b}}\nconst   arr=[1,2,3].map(  (x)  =>  x*{}  )\n",
            i, i, i, i
        );
        fs::write(repo.join(format!("src/{}", name)), content).unwrap();
    }
}

fn setup_nested_unformatted(repo: &Path) {
    let dirs = ["src/components", "src/utils", "src/hooks", "src/services"];
    for dir in &dirs {
        fs::create_dir_all(repo.join(dir)).unwrap();
    }

    let files = [
        (
            "src/components/Button.js",
            "const   Button   =   ()  =>  {return    'click'}\n",
        ),
        (
            "src/components/Modal.js",
            "const   Modal   =   ()  =>  {return    'modal'}\n",
        ),
        (
            "src/components/Header.js",
            "const   Header   =   ()  =>  {return    'header'}\n",
        ),
        (
            "src/utils/format.js",
            "function   formatDate(  d  ){return    d.toString(  )}\n",
        ),
        (
            "src/utils/validate.js",
            "function   validate(  x  ){return    x!=null}\n",
        ),
        (
            "src/hooks/useAuth.js",
            "function   useAuth(  ){return    {user:null}}\n",
        ),
        (
            "src/services/api.js",
            "async function   fetchData(  url  ){return    fetch(  url  )}\n",
        ),
        (
            "src/services/cache.js",
            "const   cache   =   new   Map(  )\nfunction   get(  k  ){return    cache.get(  k  )}\n",
        ),
    ];

    for (path, content) in &files {
        fs::write(repo.join(path), content).unwrap();
    }
}
