/// Parsed fields from a single git commit record.
///
/// Produced by `parse_commit_fields` from a NUL-delimited format string
/// `%h\x00%D\x00%aI\x00%an\x00%s\x00%b`.
pub struct CommitFields {
    pub hash: String,
    pub decorations: Vec<String>,
    pub date: String,
    pub author: String,
    pub subject: String,
    pub body: Option<String>,
}

/// Parse a single commit record from the NUL-delimited format produced by
/// `--format=%x01%h%x00%D%x00%aI%x00%an%x00%s%x00%b`.
///
/// `raw` is the text after the leading `\x01` delimiter (i.e. one chunk from
/// splitting on `\x01`).  Returns `None` if the chunk is malformed (fewer
/// than 6 NUL-separated fields or an empty hash).
pub fn parse_commit_fields(raw: &str) -> Option<CommitFields> {
    let fields: Vec<&str> = raw.splitn(6, '\x00').collect();
    if fields.len() < 6 {
        return None;
    }

    let hash = fields[0].trim_start_matches('\n').to_string();
    if hash.is_empty() {
        return None;
    }

    let decorations = parse_decorations(fields[1]);
    let date = parse_date(fields[2]);
    let author = fields[3].to_string();
    let subject = fields[4].to_string();

    let body_raw = fields[5].trim();
    let body = if body_raw.is_empty() {
        None
    } else {
        Some(body_raw.to_string())
    };

    Some(CommitFields {
        hash,
        decorations,
        date,
        author,
        subject,
        body,
    })
}

/// Format a commit as a single summary line.
///
/// Output: `* {hash} ({decorations}) {date} [{author}] {subject}\n`
/// When there are no decorations the `(...)` part is omitted.
pub fn format_commit_oneline(fields: &CommitFields) -> String {
    let dec_part = if fields.decorations.is_empty() {
        String::new()
    } else {
        format!(" ({})", fields.decorations.join(", "))
    };

    format!(
        "* {}{} {} [{}] {}\n",
        fields.hash, dec_part, fields.date, fields.author, fields.subject
    )
}

/// Indent each line of a commit body by 2 spaces.
///
/// Returns a `String` with every line of `body` prefixed with `"  "` and
/// terminated with `\n`.
pub fn format_commit_body(body: &str) -> String {
    let mut output = String::new();
    for line in body.lines() {
        output.push_str(&format!("  {}\n", line));
    }
    output
}

/// Extract the date portion (first 10 characters) from an ISO 8601 timestamp.
///
/// Returns the full string unchanged when it is shorter than 10 characters.
pub fn parse_date(iso: &str) -> String {
    if iso.len() >= 10 {
        iso[..10].to_string()
    } else {
        iso.to_string()
    }
}

/// Split a raw `%D` decoration string into individual decoration tokens.
///
/// Returns an empty `Vec` when the input is blank.  Each token is trimmed of
/// surrounding whitespace and empty tokens are dropped.
pub fn parse_decorations(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    raw.split(", ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_date ---

    #[test]
    fn parse_date_full_iso() {
        assert_eq!(parse_date("2024-01-15T10:00:00+00:00"), "2024-01-15");
    }

    #[test]
    fn parse_date_short_passthrough() {
        assert_eq!(parse_date("2024-01"), "2024-01");
    }

    #[test]
    fn parse_date_empty() {
        assert_eq!(parse_date(""), "");
    }

    // --- parse_decorations ---

    #[test]
    fn parse_decorations_empty() {
        assert!(parse_decorations("").is_empty());
        assert!(parse_decorations("   ").is_empty());
    }

    #[test]
    fn parse_decorations_single() {
        assert_eq!(parse_decorations("HEAD -> main"), vec!["HEAD -> main"]);
    }

    #[test]
    fn parse_decorations_multiple() {
        assert_eq!(
            parse_decorations("HEAD -> main, origin/main, tag: v1.0"),
            vec!["HEAD -> main", "origin/main", "tag: v1.0"]
        );
    }

    #[test]
    fn parse_decorations_trims_whitespace() {
        assert_eq!(
            parse_decorations("  HEAD -> main , origin/main  "),
            vec!["HEAD -> main", "origin/main"]
        );
    }

    // --- parse_commit_fields ---

    fn make_raw(
        hash: &str,
        decs: &str,
        date: &str,
        author: &str,
        subject: &str,
        body: &str,
    ) -> String {
        format!(
            "{}\x00{}\x00{}\x00{}\x00{}\x00{}",
            hash, decs, date, author, subject, body
        )
    }

    #[test]
    fn parse_commit_fields_basic() {
        let raw = make_raw(
            "a1b2c3f",
            "HEAD -> main",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Add feature",
            "",
        );
        let f = parse_commit_fields(&raw).unwrap();
        assert_eq!(f.hash, "a1b2c3f");
        assert_eq!(f.decorations, vec!["HEAD -> main"]);
        assert_eq!(f.date, "2024-01-15");
        assert_eq!(f.author, "Alice");
        assert_eq!(f.subject, "Add feature");
        assert!(f.body.is_none());
    }

    #[test]
    fn parse_commit_fields_with_body() {
        let body = "This is the body.\nWith multiple lines.";
        let raw = make_raw(
            "a1b2c3f",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Subject",
            body,
        );
        let f = parse_commit_fields(&raw).unwrap();
        assert_eq!(f.body, Some(body.to_string()));
    }

    #[test]
    fn parse_commit_fields_empty_body_trimmed() {
        let raw = make_raw(
            "a1b2c3f",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Subject",
            "   \n  ",
        );
        let f = parse_commit_fields(&raw).unwrap();
        assert!(f.body.is_none());
    }

    #[test]
    fn parse_commit_fields_strips_leading_newline_from_hash() {
        let raw = format!("\na1b2c3f\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Subject\x00");
        let f = parse_commit_fields(&raw).unwrap();
        assert_eq!(f.hash, "a1b2c3f");
    }

    #[test]
    fn parse_commit_fields_too_few_fields() {
        assert!(parse_commit_fields("hash\x00decs\x00date").is_none());
    }

    #[test]
    fn parse_commit_fields_empty_hash() {
        let raw = make_raw("", "", "2024-01-15T10:00:00+00:00", "Alice", "Subject", "");
        assert!(parse_commit_fields(&raw).is_none());
    }

    // --- format_commit_oneline ---

    #[test]
    fn format_oneline_with_decorations() {
        let fields = CommitFields {
            hash: "a1b2c3f".to_string(),
            decorations: vec!["HEAD -> main".to_string()],
            date: "2024-01-15".to_string(),
            author: "John Smith".to_string(),
            subject: "Add auth".to_string(),
            body: None,
        };
        assert_eq!(
            format_commit_oneline(&fields),
            "* a1b2c3f (HEAD -> main) 2024-01-15 [John Smith] Add auth\n"
        );
    }

    #[test]
    fn format_oneline_no_decorations() {
        let fields = CommitFields {
            hash: "a1b2c3f".to_string(),
            decorations: vec![],
            date: "2024-01-15".to_string(),
            author: "John Smith".to_string(),
            subject: "Fix bug".to_string(),
            body: None,
        };
        let result = format_commit_oneline(&fields);
        assert!(!result.contains('('));
        assert_eq!(result, "* a1b2c3f 2024-01-15 [John Smith] Fix bug\n");
    }

    #[test]
    fn format_oneline_multiple_decorations() {
        let fields = CommitFields {
            hash: "a1b2c3f".to_string(),
            decorations: vec![
                "HEAD -> main".to_string(),
                "origin/main".to_string(),
                "tag: v1.0".to_string(),
            ],
            date: "2024-01-15".to_string(),
            author: "Alice".to_string(),
            subject: "Release".to_string(),
            body: None,
        };
        assert_eq!(
            format_commit_oneline(&fields),
            "* a1b2c3f (HEAD -> main, origin/main, tag: v1.0) 2024-01-15 [Alice] Release\n"
        );
    }

    // --- format_commit_body ---

    #[test]
    fn format_body_single_line() {
        assert_eq!(format_commit_body("Hello world"), "  Hello world\n");
    }

    #[test]
    fn format_body_multiple_lines() {
        let result = format_commit_body("Line one\nLine two");
        assert_eq!(result, "  Line one\n  Line two\n");
    }

    #[test]
    fn format_body_empty() {
        assert_eq!(format_commit_body(""), "");
    }
}
