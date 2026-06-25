use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOp {
    Fetch,
    Pull,
    PullRebase,
    Push,
    PushSetUpstream, // push -u origin <branch>
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RemoteResult {
    pub operation: RemoteOp,
    pub success: bool,
    pub message: String,
}

/// Execute a remote git operation in a background thread
pub fn spawn_remote_op(
    workdir: PathBuf,
    op: RemoteOp,
    branch: Option<String>,
) -> std::thread::JoinHandle<RemoteResult> {
    std::thread::spawn(move || {
        let args: Vec<String> = match op {
            RemoteOp::Fetch => vec!["fetch".to_string()],
            RemoteOp::Pull => vec!["pull".to_string()],
            RemoteOp::PullRebase => vec!["pull".to_string(), "--rebase".to_string()],
            RemoteOp::Push => vec!["push".to_string()],
            RemoteOp::PushSetUpstream => {
                let b = branch.unwrap_or_default();
                vec![
                    "push".to_string(),
                    "-u".to_string(),
                    "origin".to_string(),
                    b,
                ]
            }
        };
        let output = Command::new("git")
            .args(&args)
            .current_dir(&workdir)
            .output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let msg = if out.status.success() {
                    if stdout.trim().is_empty() {
                        "done".to_string()
                    } else {
                        stdout
                    }
                } else {
                    stderr
                };
                RemoteResult {
                    operation: op,
                    success: out.status.success(),
                    message: msg,
                }
            }
            Err(e) => RemoteResult {
                operation: op,
                success: false,
                message: e.to_string(),
            },
        }
    })
}

impl std::fmt::Display for RemoteOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteOp::Fetch => write!(f, "fetch"),
            RemoteOp::Pull => write!(f, "pull"),
            RemoteOp::PullRebase => write!(f, "pull --rebase"),
            RemoteOp::Push => write!(f, "push"),
            RemoteOp::PushSetUpstream => write!(f, "push -u"),
        }
    }
}
