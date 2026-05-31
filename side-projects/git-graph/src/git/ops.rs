use anyhow::{Context, Result};
use git2::Oid;
use std::process::Command;

use super::repo::GitRepo;

impl GitRepo {
    pub fn checkout(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["checkout", &short])
    }

    /// 按分支名 checkout
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["checkout", name])
    }

    pub fn merge(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["merge", &short])
    }

    pub fn cherry_pick(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["cherry-pick", &short])
    }

    #[allow(dead_code)]
    pub fn reset_soft(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["reset", "--soft", &short])
    }

    pub fn reset_hard(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["reset", "--hard", &short])
    }

    #[allow(dead_code)]
    pub fn create_tag(&self, oid: Oid, name: &str) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["tag", name, &short])
    }

    pub fn create_branch(&self, oid: Oid, name: &str) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["branch", name, &short])
    }

    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.run_git(&["tag", "-d", name])
    }

    pub fn push_tag(&self, name: &str) -> Result<()> {
        self.run_git(&["push", "origin", name])
    }

    pub fn delete_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["branch", "-D", name])
    }

    /// 将文件添加到暂存区（git add <path>）
    pub fn stage_file(&self, path: &str) -> Result<()> {
        self.run_git(&["add", path])
    }

    /// 将文件从暂存区移回工作区（git restore --staged <path>）
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        self.run_git(&["restore", "--staged", path])
    }

    /// 丢弃工作区修改（git restore <path>）
    pub fn discard_file(&self, path: &str) -> Result<()> {
        self.run_git(&["restore", path])
    }

    fn workdir(&self) -> Result<&std::path::Path> {
        self.repo().workdir().context("bare 仓库不支持此操作")
    }

    fn run_git(&self, args: &[&str]) -> Result<()> {
        let workdir = self.workdir()?;
        let output = Command::new("git")
            .args(args)
            .current_dir(workdir)
            .output()
            .with_context(|| format!("执行 git {} 失败", args.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git {} 失败: {}", args.join(" "), stderr);
        }
        Ok(())
    }
}
