use std::process::Command;

use crate::commit::{parse_co_authors, CommitType, FileChange, ParsedCommit};

/// Build the `git log` command with time window and format.
fn build_git_command(repo: &str, since: &str, until: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.args([
        "-C",
        repo,
        "log",
        &format!("--since={}", since),
        &format!("--until={}", until),
        "--format=@@COMMIT@@%n%H%n%an%n%ae%n%s%n%b%n@@NUMSTAT@@",
        "--numstat",
        "--no-merges",
        "--no-renames",
    ]);
    cmd
}

/// Run git log and parse output into Vec<ParsedCommit>.
pub fn fetch_commits(repo: &str, since: &str, until: &str) -> Result<Vec<ParsedCommit>, String> {
    let output = build_git_command(repo, since, until)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {}", stderr));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    parse_log_output(&raw)
}

/// Parse the raw git log output into structured commits.
fn parse_log_output(raw: &str) -> Result<Vec<ParsedCommit>, String> {
    let mut commits = Vec::new();
    for block in raw.split("@@COMMIT@@").skip(1) {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let commit = parse_single_commit(block)?;
        commits.push(commit);
    }
    Ok(commits)
}

/// Parse one commit block into ParsedCommit.
///
/// Block format:
/// <hash>
/// <author_name>
/// <author_email>
/// <subject>
/// <body lines...>
/// @@NUMSTAT@@
/// <added>\t<deleted>\t<file>
/// ...
fn parse_single_commit(block: &str) -> Result<ParsedCommit, String> {
    let numstat_pos = block
        .rfind("@@NUMSTAT@@")
        .ok_or_else(|| "Missing @@NUMSTAT@@ marker in block".to_string())?;
    let header = block[..numstat_pos].trim();
    let numstat = block[numstat_pos + "@@NUMSTAT@@".len()..].trim();

    let mut header_lines = header.lines();

    let hash = header_lines
        .next()
        .ok_or("Missing hash")?
        .trim()
        .to_string();
    let author_name = header_lines
        .next()
        .ok_or("Missing author name")?
        .trim()
        .to_string();
    let author_email = header_lines
        .next()
        .ok_or("Missing author email")?
        .trim()
        .to_string();
    let subject = header_lines
        .next()
        .ok_or("Missing subject")?
        .trim()
        .to_string();
    let body_str = header_lines.collect::<Vec<_>>().join("\n");

    let commit_type = CommitType::from_subject(&subject);
    let co_authors = parse_co_authors(&body_str);

    let files: Vec<FileChange> = numstat
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 2 {
                return None;
            }
            let added: u64 = parts[0].parse().unwrap_or(0);
            let deleted: u64 = parts[1].parse().unwrap_or(0);
            Some(FileChange { added, deleted })
        })
        .collect();

    Ok(ParsedCommit {
        hash,
        author_name,
        author_email,
        subject,
        commit_type,
        co_authors,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_commit_basic() {
        let block = "\
abc123
Alice
alice@corp.com
feat: add login
Implemented the login flow

@@NUMSTAT@@
10\t5\tsrc/auth.rs
3\t0\tsrc/main.rs
";
        let commit = parse_single_commit(block).unwrap();
        assert_eq!(commit.hash, "abc123");
        assert_eq!(commit.author_name, "Alice");
        assert_eq!(commit.author_email, "alice@corp.com");
        assert_eq!(commit.subject, "feat: add login");
        assert_eq!(commit.commit_type, CommitType::Feat);
        assert_eq!(commit.files.len(), 2);
        assert_eq!(commit.files[0].added, 10);
        assert_eq!(commit.files[0].deleted, 5);
        assert_eq!(commit.files[1].added, 3);
        assert_eq!(commit.files[1].deleted, 0);
    }

    #[test]
    fn test_parse_single_commit_with_co_author() {
        let block = "\
def456
Bob
bob@corp.com
fix: crash on null
Null check added

Co-Authored-By: Alice <alice@corp.com>
@@NUMSTAT@@
2\t2\tsrc/lib.rs
";
        let commit = parse_single_commit(block).unwrap();
        assert_eq!(commit.co_authors.len(), 1);
        assert_eq!(commit.co_authors[0].name, "Alice");
    }

    #[test]
    fn test_parse_single_commit_empty_numstat() {
        let block = "\
ghi789
Eve
eve@corp.com
chore: bump version
@@NUMSTAT@@
";
        let commit = parse_single_commit(block).unwrap();
        assert_eq!(commit.files.len(), 0);
    }

    #[test]
    fn test_parse_commit_body_contains_sentinel() {
        let block = "\
jkl012
Dave
dave@corp.com
fix: handle edge case
Fixed the @@NUMSTAT@@ parsing bug.
Also mentioned @@COMMIT@@ in the code review.
@@NUMSTAT@@
5\t3\tsrc/parser.rs
";
        let commit = parse_single_commit(block).unwrap();
        assert_eq!(commit.hash, "jkl012");
        assert_eq!(commit.files.len(), 1);
        assert_eq!(commit.files[0].added, 5);
    }
}
