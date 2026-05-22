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
    // Split on @@COMMIT@@ to get each commit block
    let blocks: Vec<&str> = raw.split("@@COMMIT@@").collect();

    for block in blocks.iter().skip(1) {
        // skip empty first split
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        if let Some(commit) = parse_single_commit(block) {
            commits.push(commit);
        }
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
fn parse_single_commit(block: &str) -> Option<ParsedCommit> {
    // Split at @@NUMSTAT@@ marker
    let parts: Vec<&str> = block.splitn(2, "@@NUMSTAT@@").collect();
    let header = parts.first()?.trim();
    let numstat = parts.get(1).map(|s| s.trim()).unwrap_or("");

    let mut header_lines = header.lines();

    let hash = header_lines.next()?.trim().to_string();
    let author_name = header_lines.next()?.trim().to_string();
    let author_email = header_lines.next()?.trim().to_string();
    let subject = header_lines.next()?.trim().to_string();
    // Remaining lines are the body
    let body: Vec<&str> = header_lines.collect();
    let body_str = body.join("\n");

    let commit_type = CommitType::from_subject(&subject);
    let co_authors = parse_co_authors(&body_str);

    // Parse numstat lines: <added>\t<deleted>\t<filename>
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

    Some(ParsedCommit {
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
}
