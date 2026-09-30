//! 确定性策略层（Jev 之前跑）。
//!
//! 本模块的一切都在调用 Jev 之前执行。硬黑名单是**刻意不可覆盖**的：
//! 概率判定绝不能触及它们，否则一次校准失误就可能变成批准 `rm -rf /`。
//!
//! 设计参照 `jomatsu/pi-jev-auto-mode` 的 `policy.ts`（MIT），并适配 cc-code。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

// ─── 通用匹配工具 ───────────────────────────────────────────────────────────

/// Shell 控制语法。允许 pattern **绝不能**匹配穿过这些字符的命令，
/// 否则 `ls*` 会放行 `ls && rm -rf /`。
fn has_shell_control(cmd: &str) -> bool {
    cmd.chars().any(|c| {
        matches!(
            c,
            '\r' | '\n' | ';' | '&' | '|' | '<' | '>' | '$' | '`' | '(' | ')' | '\\'
        )
    })
}

fn has_path_glob(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c, '*' | '?' | '[' | ']' | '{' | '}'))
}

/// 把 shell 风格 `*` / `?` 通配符编译为正则（大小写不敏感）。
fn glob_to_regex(pattern: &str) -> Option<Regex> {
    let mut src = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => src.push_str("[\\s\\S]*"),
            '?' => src.push_str("[\\s\\S]"),
            c => src.push_str(&regex::escape(&c.to_string())),
        }
    }
    src.push('$');
    Regex::new(&src).ok()
}

fn matches_pattern(cmd: &str, pattern: &str, allow_shell_control: bool) -> bool {
    let pat = pattern.trim();
    if pat.is_empty() {
        return false;
    }
    if !allow_shell_control && has_shell_control(cmd) {
        return false;
    }
    match glob_to_regex(pat) {
        Some(re) => re.is_match(cmd.trim()),
        None => false,
    }
}

fn matches_any(cmd: &str, patterns: &[String], allow_shell_control: bool) -> Option<String> {
    patterns
        .iter()
        .find(|p| matches_pattern(cmd, p, allow_shell_control))
        .cloned()
}

// ─── 硬黑名单（永不进 Jev）──────────────────────────────────────────────────

struct HardDeny {
    name: &'static str,
    re: Regex,
}

static HARD_DENY: LazyLock<Vec<HardDeny>> = LazyLock::new(|| {
    let mk = |name, pat: &str| HardDeny {
        name,
        re: Regex::new(pat).expect("hard-deny regex"),
    };
    vec![
        // 递归删除系统/家目录根
        // 末尾的 `\*` 很关键：`rm -rf /*` 与 `rm -rf /` 等价，必须同罪。
        // 根目录轮的盘符形式 `C:\` / `C:/` 也要覆盖。
        mk(
            "recursive delete of a system or home root",
            r#"(?i)\brm\b[^\n;&|]*(?:--recursive|-[^\s;&|]*[rR][^\s;&|]*)[^\n;&|]*\s+["']?(?:/|~|\$HOME|\$\{HOME\}|/(?:Users|home|root|System|Applications|Library|etc|usr|var|bin|sbin|opt|private|Volumes)|[A-Za-z]:[\\/])(?:["']?(?:\s|$)|\/|\*)"#,
        ),
        // 递归删除但目标**静态不可判断**（命令替换 / 变量 / 反引号 / 别的用户家目录）
        //
        // 注意：普通 glob（`*` `?` `[]` `{}`）**不算**不可判断——`rm -rf build/*`
        // 的目标是明确的仓库内路径。早前把这些字符也列进来，导致日常命令
        // （`rm -rf node_modules/*`）落进不可覆盖的硬黑名单、无法申诉。
        mk(
            "unresolved recursive delete target",
            r#"(?i)\brm\b[^\n;&|]*(?:--recursive|-[^\s;&|]*[rR][^\s;&|]*)[^\n;&|]*(?:\$\(|\$\{|\$[A-Za-z_]|~[A-Za-z]|`)[^\n;&|]*"#,
        ),
        // ── 下载执行（把远端脚本直接交给 shell）──
        // 这是最经典的 RCE，**必须落在确定性清单里**，不能只靠 Jev 的概率判定。
        // 官方（OpenRouter cookbook）明确要求把这类放静态清单，且要先跑：
        // "commands that hand a quoted string to another shell or interpreter,
        //  which is the one way to hide any of the above from a text check."
        //
        // 审计发现：此前 `curl|bash` 及其**全部 8 个常见变体**都不在硬黑名单里，
        // 只挂在 DANGEROUS 上（升级 Jev）——等于用概率墙守 RCE。
        // 下面按变体逐条覆盖：包装命令（sudo/env/command/nohup/timeout）、
        // 路径前缀（/bin/bash）、进程替换（bash <(curl…)）、命令替换（sh -c "$(curl…)"）。
        // 审计补丁 P0：`| xargs sh` 这类也要覆盖（官方 RISKY 把 xargs 与 sh 并列）
        mk(
            "downloaded script execution",
            r"(?i)\b(?:curl|wget)\b[^\n;&|]*\|\s*(?:sudo\s+|env\s+|command\s+|exec\s+|nohup\s+|timeout\s+\S+\s+|xargs\s+)*[^\s;&|]*(?:sh|bash|zsh|dash|ksh|fish)\b",
        ),
        // 审计补丁 P0：把远端内容交给**别的解释器**执行。
        // 官方 RISKY 列表把 `^(bash|sh|zsh|fish|eval|xargs)\b` 并列，并单列
        // `python -c / node -e` —— 此前只覆盖 sh/bash/zsh，`eval "$(curl …)"`
        // 和 `python3 -c "$(curl …)"` 全是漏的（实测 5/7 漏拦）。
        mk(
            "downloaded script execution via interpreter",
            r"(?i)\b(?:sh|bash|zsh|dash|ksh|fish|eval|python3?|node|perl|ruby|php|deno|bun|xargs)\b[^\n;&|]*(?:\$\(|`)[^\n;&|]*(?:curl|wget)\b",
        ),
        mk(
            "downloaded script execution via process substitution",
            r"(?i)\b(?:sh|bash|zsh|dash|ksh|fish)\b[^\n;&|]*<\s*\(\s*(?:curl|wget)\b",
        ),
        mk(
            "downloaded script execution via command substitution",
            r#"(?i)\b(?:sh|bash|zsh|dash|ksh|fish)\b[^\n;&|]*(?:-c\b[^\n;&|]*)?(?:\$\(|`)[^\n;&|]*(?:curl|wget)\b"#,
        ),
        // 格式化 / 擦除文件系统
        mk(
            "filesystem format or signature wipe",
            r"(?i)\b(?:mkfs(?:\.[a-z0-9_+-]+)?|wipefs)\b",
        ),
        // dd 写裸设备
        mk(
            "disk device overwrite",
            r#"(?i)\bdd\b[^\n;&|]*\bof\s*=\s*["']?/dev/"#,
        ),
        // macOS 磁盘擦除/分区
        mk(
            "macOS disk erase or partition",
            r"(?i)\bdiskutil\b[^\n;&|]*\b(?:erase|partition|apfs\s+delete|apfs\s+erase)\b",
        ),
        // 强推保护分支（两种情况都覆盖）
        mk(
            "forced push to a protected branch",
            r"(?i)\bgit\b[^\n;&|]*\bpush\b[^\n;&|]*(?:--force(?:-with-lease)?|-[^\s;&|]*f[^\s;&|]*)\b[^\n;&|]*\b(?:main|master|production|prod)\b",
        ),
        mk(
            "forced push to a protected branch",
            r"(?i)\bgit\b[^\n;&|]*\bpush\b[^\n;&|]*\b(?:main|master|production|prod)\b[^\n;&|]*(?:--force(?:-with-lease)?|-[^\s;&|]*f[^\s;&|]*)\b",
        ),
        // 未解析的强推目标（同上：普通 glob 不算不可判断，`feature/*` 是明确分支模式）
        mk(
            "unresolved forced push target",
            r#"(?i)\bgit\b[^\n;&|]*\bpush\b[^\n;&|]*(?:--force(?:-with-lease)?|-[^\s;&|]*f[^\s;&|]*)[^\n;&|]*(?:\$\(|\$\{|\$[A-Za-z_]|~[A-Za-z]|`)[^\n;&|]*"#,
        ),
    ]
});

/// 命中硬黑名单则返回原因名；永不交给语义层。
pub fn hard_deny_reasons(command: &str) -> Vec<&'static str> {
    HARD_DENY
        .iter()
        .filter(|h| h.re.is_match(command))
        .map(|h| h.name)
        .collect()
}

// ─── 危险形状（命中 → 升级到 Jev）────────────────────────────────────────────

struct Dangerous {
    name: &'static str,
    re: Regex,
}

static DANGEROUS: LazyLock<Vec<Dangerous>> = LazyLock::new(|| {
    let mk = |name, pat: &str| Dangerous {
        name,
        re: Regex::new(pat).expect("dangerous regex"),
    };
    vec![
        // 文件删除 / 破坏性遍历
        // 注：Rust regex 无 look-ahead，直接匹配"rm ... 含 -r/-f 组合或 --recursive"。
        mk(
            "recursive/forced rm",
            r"(?i)\brm\b[^\n;&|]*(?:\s-[^\s;&|]*[rR][^\s;&|]*[fF]|\s-[^\s;&|]*[fF][^\s;&|]*[rR]|\s-[^\s;&|]*[rR]\b|\s--recursive\b)",
        ),
        mk(
            "remove Git metadata",
            r#"(?i)\brm\b[^\n;&|]*\s(?:\.git|\.git/|['"]\.git['"])"#,
        ),
        mk("find delete", r"(?i)\bfind\b[^\n;&|]*\s-delete\b"),
        mk(
            "find exec",
            r"(?i)\bfind\b[^\n;&|]*\s-(?:exec|execdir|ok|okdir)\b",
        ),
        mk("xargs rm", r"(?i)\bxargs\b[^\n;&|]*\brm\b"),
        // 包执行/发布（可运行第三方代码或改动远端状态）
        mk(
            "package execution or publish",
            r"(?i)\b(?:npm|pnpm|yarn|bun|pip|pip3|uv|poetry|cargo|gem|go|brew|apt(?:-get)?|dnf|pacman)\b[^\n;&|]*\b(?:exec|run|dlx|publish)\b",
        ),
        mk(
            "package runner",
            r"(?i)\b(?:npx|pnpm\s+dlx|yarn\s+dlx|bunx|pipx|uvx)\b",
        ),
        // 提权 / 权限与归属
        mk("sudo", r"(?i)\bsudo\b"),
        mk(
            "world-writable permissions",
            r"(?i)\bchmod\b[^\n;&|]*\b777\b",
        ),
        mk(
            "recursive chmod/chown",
            r"(?i)\b(?:chmod|chown)\b[^\n;&|]*\s(?:-R|--recursive)\b",
        ),
        // 磁盘 / 分区 / 文件系统
        mk("format filesystem", r"(?i)\bmkfs(?:\.[a-z0-9_+-]+)?\b"),
        mk("wipe filesystem signatures", r"(?i)\bwipefs\b"),
        mk("disk shred/wipe", r"(?i)\b(?:shred|srm)\b"),
        mk(
            "partition editor",
            r"(?i)\b(?:fdisk|parted|gparted|sfdisk|cfdisk)\b",
        ),
        mk(
            "macOS disk erase",
            r"(?i)\bdiskutil\b[^\n;&|]*\b(?:erase|partition|apfs\s+delete|apfs\s+erase)\b",
        ),
        mk("dd writes to disk device", r"(?i)\bdd\b[^\n;&|]*\bof=/dev/"),
        // Git 工作树 / 仓库 / 历史破坏
        mk(
            "git reset hard",
            r"(?i)\bgit\b[^\n;&|]*\breset\b[^\n;&|]*\s--hard\b",
        ),
        mk(
            "git clean forced",
            r"(?i)\bgit\b[^\n;&|]*\bclean\b[^\n;&|]*\s-[^\s;&|]*f[^\s;&|]*",
        ),
        mk(
            "git force push",
            r"(?i)\bgit\b[^\n;&|]*\bpush\b[^\n;&|]*\s--(?:force|force-with-lease|mirror)\b",
        ),
        mk(
            "git force push",
            r"(?i)\bgit\b[^\n;&|]*\bpush\b[^\n;&|]*\s-[^\s;&|]*f[^\s;&|]*\b",
        ),
        mk(
            "git branch force-delete",
            r"(?i)\bgit\b[^\n;&|]*\bbranch\b[^\n;&|]*\s-D\b",
        ),
        mk(
            "git tag delete",
            r"(?i)\bgit\b[^\n;&|]*\btag\b[^\n;&|]*\s-d\b",
        ),
        mk("git remove files", r"(?i)\bgit\b[^\n;&|]*\brm\b"),
        mk(
            "git checkout all files",
            r"(?i)\bgit\b[^\n;&|]*\bcheckout\b[^\n;&|]*\s--\s+(?:\.|\*)\b",
        ),
        mk(
            "git restore all files",
            r"(?i)\bgit\b[^\n;&|]*\brestore\b[^\n;&|]*(?:\s\.\b|\s:/\b|\s--source\b)",
        ),
        mk(
            "git reflog expiry",
            r"(?i)\bgit\b[^\n;&|]*\breflog\b[^\n;&|]*\bexpire\b",
        ),
        mk(
            "git aggressive prune/gc",
            r"(?i)\bgit\b[^\n;&|]*\b(?:gc|prune)\b[^\n;&|]*(?:--prune=(?:now|all)|--expire\s+now|--expire=now)",
        ),
        // 容器 / 卷
        mk(
            "docker prune/remove volumes",
            r"(?i)\bdocker\b[^\n;&|]*\b(?:system\s+prune|volume\s+(?:rm|prune)|container\s+prune|image\s+prune)\b",
        ),
        mk(
            "docker compose remove volumes",
            r"(?i)\bdocker\s+compose\b[^\n;&|]*\bdown\b[^\n;&|]*(?:\s-v\b|\s--volumes\b)",
        ),
        // 运行远端脚本 = 把当前用户权限交给脚本作者
        mk(
            "downloaded script execution",
            r"(?i)\b(?:curl|wget)\b[^\n;&|]*(?:\|\s*(?:sh|bash|zsh)\b|\b(?:sh|bash|zsh)\s*<\s*\()",
        ),
        // 外发本地数据
        mk(
            "network upload of local data",
            r"(?i)\b(?:curl|wget)\b[^\n;&|]*(?:\s-d\s*@|\s--data(?:-binary|-raw|-urlencode)?\s*@|\s-T\s|\s--upload-file\b|\s-F\s[^\s;&|]*=@|\s--form\s[^\s;&|]*=@)",
        ),
        mk(
            "file transfer to a remote host",
            r"(?i)\b(?:scp|rsync|sftp)\b",
        ),
        mk(
            "raw network connection",
            r"(?i)\b(?:nc|ncat|netcat|telnet)\b",
        ),
        // 读凭据进上下文（无副作用，但私钥进了对话就是长期泄漏）
        mk(
            "reads a credential file",
            r"(?i)\b(?:cat|bat|less|more|head|tail|xxd|base64|grep|rg)\b[^\n;&|]*(?:\.ssh/|id_rsa|id_ed25519|id_ecdsa|\.aws/|\.gnupg|\.npmrc|credentials|\.env\b)",
        ),
        // ── Windows 破坏性命令（宿主为 Windows，Bash 走 cmd /C / PowerShell）──
        mk(
            "windows recursive delete",
            r"(?i)\b(?:del|erase)\b[^\n;&|]*/[a-z]*(?:s|q)[a-z]*\b",
        ),
        mk(
            "windows remove directory tree",
            r"(?i)\b(?:rd|rmdir)\b[^\n;&|]*/[a-z]*s[a-z]*\b",
        ),
        mk("windows format drive", r"(?i)\bformat\b\s+[a-z]:"),
        mk("windows diskpart", r"(?i)\bdiskpart\b"),
        mk("windows registry delete", r"(?i)\breg\b[^\n;&|]*\bdelete\b"),
        mk(
            "powershell recursive force delete",
            r"(?i)\bRemove-Item\b[^\n;&|]*-[^\n;&|]*(?:Recurse|Force)\b",
        ),
        mk("powershell clear disk", r"(?i)\bClear-Disk\b"),
        mk(
            "windows ownership/permission takeover",
            r"(?i)\b(?:takeown|icacls)\b",
        ),
        mk("windows mirror copy", r"(?i)\brobocopy\b[^\n;&|]*\s/mir\b"),
    ]
});

/// 命中危险 pattern 的原因名（空 = 确定性层认为安全，走快车道）。
pub fn dangerous_reasons(command: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = DANGEROUS
        .iter()
        .filter(|d| d.re.is_match(command))
        .map(|d| d.name)
        .collect();
    // 后置过滤：仅涉及 .env.example 等模板文件的读取不算凭据读取
    if out.contains(&"reads a credential file") && only_template_credentials(command) {
        out.retain(|r| *r != "reads a credential file");
    }
    out.dedup();
    out
}

/// 命令中出现的凭据类路径是否**全部**是模板文件（`.env.example` 等）。
///
/// 按 token 逐个判断：只要有一个 `.env`（无模板后缀）或真实凭据路径，就不能豁免。
/// 原实现只要命令里**出现**模板名就整体豁免，导致 `cat .env.example .env` 放行。
fn only_template_credentials(command: &str) -> bool {
    const TEMPLATE_SUFFIXES: &[&str] =
        &[".env.example", ".env.sample", ".env.template", ".env.dist"];
    const REAL_FRAGMENTS: &[&str] = &[
        ".ssh/",
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        ".aws/",
        ".gnupg",
        ".npmrc",
        "credentials",
    ];
    let mut saw_env = false;
    for raw in command.split_whitespace() {
        let tok = raw
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_lowercase();
        if REAL_FRAGMENTS.iter().any(|f| tok.contains(f)) {
            return false; // 出现真实凭据路径
        }
        if tok.contains(".env") {
            saw_env = true;
            if !TEMPLATE_SUFFIXES.iter().any(|s| tok.ends_with(s)) {
                return false; // 出现真实 .env（非模板）
            }
        }
    }
    saw_env
}

// ─── 范围化本地删除（`rm -rf build` 之类）───────────────────────────────────

const LOCAL_DELETION_REASONS: &[&str] = &["recursive/forced rm", "find delete"];

fn is_safe_relative_target(target: &str, cwd: &Path, allow_cwd_itself: bool) -> bool {
    if target.is_empty() {
        return false;
    }
    if target.starts_with('/') || target.starts_with('~') || target.starts_with('$') {
        return false;
    }
    // Windows 盘符绝对路径
    let chars: Vec<char> = target.chars().take(3).collect();
    if chars.len() >= 2
        && chars[0].is_ascii_alphabetic()
        && chars[1] == ':'
        && matches!(chars.get(2), Some('\\') | Some('/'))
    {
        return false;
    }
    if has_path_glob(target) {
        return false;
    }
    let stripped = target.trim_start_matches("./");
    let segs: Vec<&str> = stripped.split(['/', '\\']).collect();
    if !allow_cwd_itself && segs.len() == 1 && segs[0].is_empty() {
        return false;
    }
    if segs.iter().any(|s| *s == ".." || *s == ".git") {
        return false;
    }
    // 纯词法解析（不访问文件系统，不解析符号链接）
    let root = normalize_lexical(cwd);
    let candidate = normalize_lexical(&root.join(stripped));
    candidate.starts_with(&root) && candidate != root
}

/// 词法归一化（不访问文件系统，避免符号链接解析）。
///
/// **必须用于"路径是否在项目内"这类判断**：`Path::starts_with` 是纯词法比较，
/// `cwd/../../etc/passwd` 词法上确实以 cwd 开头，会被误判成项目内路径。
pub fn normalize_lexical(p: &Path) -> PathBuf {
    use std::path::Component::*;
    let mut out = PathBuf::new();
    let mut has_root = false;
    for comp in p.components() {
        match comp {
            Prefix(_) | RootDir => {
                out.push(comp.as_os_str());
                has_root = true;
            }
            CurDir => {}
            ParentDir => {
                if !out.pop() && !has_root {
                    out.push("..");
                }
            }
            Normal(s) => out.push(s),
        }
    }
    out
}

/// `rm -rf <相对路径>`（仓库内、非 .git、无通配符）。
pub fn is_scoped_rm(command: &str, cwd: &Path) -> bool {
    if has_shell_control(command) || has_path_glob(command) {
        return false;
    }
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.first().map(|s| s.to_lowercase()) != Some("rm".to_string()) {
        return false;
    }
    let mut options_ended = false;
    let mut targets: Vec<&str> = Vec::new();
    for tok in tokens.iter().skip(1) {
        if !options_ended && *tok == "--" {
            options_ended = true;
            continue;
        }
        if !options_ended && tok.starts_with('-') {
            continue;
        }
        // 首个 target 之后又出现选项 → 不是简单删除
        if tok.starts_with('-') {
            return false;
        }
        targets.push(tok);
    }
    !targets.is_empty()
        && targets
            .iter()
            .all(|t| is_safe_relative_target(t, cwd, false))
}

/// `find <相对路径> ... -delete`（不含 -exec）。
pub fn is_scoped_find_delete(command: &str, cwd: &Path) -> bool {
    if has_shell_control(command) || has_path_glob(command) {
        return false;
    }
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.first().map(|s| s.to_lowercase()) != Some("find".to_string()) {
        return false;
    }
    if tokens.iter().any(|t| *t == "-exec" || *t == "-execdir") {
        return false;
    }
    if !tokens.contains(&"-delete") {
        return false;
    }
    let expr_start = tokens
        .iter()
        .position(|t| t.starts_with('-') || *t == "!" || *t == "(" || *t == ")");
    let Some(start) = expr_start else {
        return false;
    };
    if start == 0 {
        return false;
    }
    let roots = &tokens[1..start];
    const NARROWING: &[&str] = &[
        "-atime",
        "-ctime",
        "-empty",
        "-group",
        "-iname",
        "-ipath",
        "-iregex",
        "-links",
        "-maxdepth",
        "-mindepth",
        "-mtime",
        "-name",
        "-newer",
        "-newermt",
        "-path",
        "-perm",
        "-regex",
        "-size",
        "-type",
        "-user",
    ];
    let has_narrowing = tokens.iter().any(|t| NARROWING.contains(t));
    !roots.is_empty()
        && roots
            .iter()
            .all(|r| is_safe_relative_target(r, cwd, has_narrowing))
}

/// 命中范围化本地删除则从危险原因中剔除。
pub fn is_scoped_local_deletion(command: &str, cwd: &Path) -> bool {
    is_scoped_rm(command, cwd) || is_scoped_find_delete(command, cwd)
}

/// 危险原因（考虑范围化本地删除豁免）。
/// 按 `&&` / `||` / `;` / 换行拆分命令链。
///
/// **刻意不拆单个 `|`**：`curl … | bash` 这类危险形状必须保持整段才能被识别；
/// 各危险 pattern 都用 `[^\n;&|]*` 保证不跨越 `&&`/`;`，故按段判定与整串等价。
pub fn split_command_chain(command: &str) -> Vec<String> {
    const SEP: char = '\u{1}'; // 临时标记，避免与命令内容冲突
    command
        .replace("||", &SEP.to_string())
        .replace("&&", &SEP.to_string())
        .split([SEP, ';', '\n'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 危险原因（**链式感知**）。
///
/// 逐段判定：某一"纯 `rm -rf <仓库内路径>` / `find … -delete`"段被识别为范围化本地删除时，
/// 只剔除**该段**的删除类原因；其他段照常判定。这样 `rm -rf build && ls` 会被正确豁免，
/// 而 `rm -rf build && rm -rf /` 仍保留危险标记（且 `/` 段本就被硬黑名单拦截）。
pub fn dangerous_reasons_scoped(command: &str, cwd: &Path) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for seg in split_command_chain(command) {
        let mut r = dangerous_reasons(&seg);
        if is_scoped_local_deletion(&seg, cwd) {
            r.retain(|x| !LOCAL_DELETION_REASONS.contains(x));
        }
        out.extend(r);
    }
    out.sort_unstable();
    out.dedup();
    out
}

// ─── 只读白名单（零成本快车道）──────────────────────────────────────────────

/// 只读检查命令。刻意**不含**任何执行项目代码的命令（如 test runner）。
/// 注意 `find` 在此列表中，但含 `-exec`/`-delete` 等谓词时由
/// [`is_read_only_command`] 剔除（见 [`find_has_dangerous_predicate`]）。
pub static SAFE_COMMANDS: &[&str] = &[
    // shell 状态与导航
    "pwd",
    "cd*",
    "ls*",
    "tree*",
    "whoami",
    "hostname",
    "uname*",
    "date",
    // 读文件与 stdin
    "cat*",
    "bat*",
    "head*",
    "tail*",
    "less*",
    "wc*",
    "file*",
    "stat*",
    "realpath*",
    "readlink*",
    "basename*",
    "dirname*",
    "du*",
    "df*",
    "find*",
    // 文本搜索与转换（不写文件）
    "grep*",
    "rg*",
    "ag*",
    "jq*",
    "diff*",
    "cmp*",
    "sort*",
    "uniq*",
    "cut*",
    "column*",
    "nl*",
    "xxd*",
    // 版本探测
    "node --version*",
    "npm --version*",
    "python --version*",
    "python3 --version*",
    "uv --version*",
    "go version*",
    "cargo --version*",
    "gh --version*",
    // git 只读子命令
    "git status*",
    "git diff*",
    "git log*",
    "git show*",
    "git branch",
    "git remote",
    "git remote -v",
    "git blame*",
    "git shortlog*",
    "git describe*",
    "git rev-parse*",
    "git ls-files*",
    "git ls-tree*",
    "git worktree list*",
    "git stash list*",
    "git tag",
];

static SAFE_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    SAFE_COMMANDS
        .iter()
        .filter_map(|p| glob_to_regex(p))
        .collect()
});

/// `find` 的执行/写入类谓词 —— 出现任一即不再视为只读。
const DANGEROUS_FIND_PREDICATES: &[&str] = &[
    "-exec", "-execdir", "-delete", "-fprint", "-fprint0", "-fprintf", "-fls", "-ok", "-okdir",
];

fn find_has_dangerous_predicate(command: &str) -> bool {
    command
        .split_whitespace()
        .any(|t| DANGEROUS_FIND_PREDICATES.contains(&t))
}

/// 单条只读命令。
pub fn is_read_only_command(command: &str) -> bool {
    if has_shell_control(command) {
        return false;
    }
    let c = command.trim();
    // `find` 带执行谓词（-exec/-delete/...）时不是只读——否则会绕过硬黑名单与 Jev。
    if (c == "find" || c.starts_with("find ") || c.starts_with("find\t"))
        && find_has_dangerous_predicate(c)
    {
        return false;
    }
    SAFE_REGEXES.iter().any(|re| re.is_match(c))
}

/// 只读命令链（`cd src && ls && git log`）——每段都只读才算。
pub fn is_read_only_chain(command: &str) -> bool {
    let segs: Vec<&str> = command
        .split(['\n'])
        .flat_map(|s| s.split("&&"))
        .flat_map(|s| s.split("||"))
        .flat_map(|s| s.split(';'))
        .flat_map(|s| s.split('|'))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    !segs.is_empty() && segs.iter().all(|s| is_read_only_command(s))
}

/// 用户声明的快车道（优先级高于危险 pattern）。
pub fn is_user_declared_safe(command: &str, safe_commands: &[String]) -> bool {
    matches_any(command, safe_commands, false).is_some()
}

// ─── 用户 allow/deny 规则 ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRuleDecision {
    Allow { pattern: String },
    Deny { pattern: String },
}

/// 公开的单层匹配入口。
///
/// 门需要**按优先级分层**评估（人写下的配置 > LLM 推断的规则 > 内置白名单），
/// 因此不能再把 allow/deny 混在一个函数里调用。
///
/// `allow_shell_control = true` 允许 pattern 穿过 `&&`/`|` 等控制符——
/// **只对 deny 规则开放**（宽松是更安全的方向）。
pub fn matches_command_pattern(
    command: &str,
    patterns: &[String],
    allow_shell_control: bool,
) -> Option<String> {
    matches_any(command, patterns, allow_shell_control)
}

/// deny 优先；deny 允许匹配含控制语法的命令（宽松是更安全的方向）。
///
/// 注：门已改用 [`matches_command_pattern`] 分层评估（因为 LLM 推断的规则
/// 必须低于人写下的 allow/safe）。本函数保留给"同一信任级内 deny 优先"的调用方。
pub fn evaluate_user_rules(
    command: &str,
    allowed: &[String],
    disallowed: &[String],
) -> Option<UserRuleDecision> {
    if let Some(p) = matches_any(command, disallowed, true) {
        return Some(UserRuleDecision::Deny { pattern: p });
    }
    if let Some(p) = matches_any(command, allowed, false) {
        return Some(UserRuleDecision::Allow { pattern: p });
    }
    None
}

// ─── 仓库现场（git）────────────────────────────────────────────────────────

/// 当前 git 分支：读 `.git/HEAD`，**不启动子进程**，零成本。
///
/// 规则常常是**有条件的**——"不要在 main 上直接提交"只在 main 上才成立。
/// judge 若不知道当前分支，这类规则就无从判定，只能去匹配命令里出现的分支名，
/// 于是 `git push origin main` 被拦、而真正在 main 上的 `git commit` 被放行。
///
/// 返回 `None` = 不是 git 仓库 / 读不到。游离 HEAD 返回短 SHA。
pub fn git_branch(cwd: &Path) -> Option<String> {
    let dot_git = cwd.join(".git");
    let head_path = if dot_git.is_dir() {
        dot_git.join("HEAD")
    } else {
        // worktree / submodule：`.git` 是文件，内容形如 `gitdir: <path>`
        let content = std::fs::read_to_string(&dot_git).ok()?;
        let dir = content.trim().strip_prefix("gitdir:")?.trim();
        let dir = Path::new(dir);
        let abs = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            cwd.join(dir)
        };
        abs.join("HEAD")
    };

    let content = std::fs::read_to_string(head_path).ok()?;
    let content = content.trim();
    match content.strip_prefix("ref:") {
        Some(r) => r.trim().strip_prefix("refs/heads/").map(str::to_string),
        // 游离 HEAD：给出短 SHA，judge 至少知道"不在任何分支上"
        None => Some(content.chars().take(7).collect()),
    }
}

// ─── 受保护路径（Write/Edit）────────────────────────────────────────────────
const PROTECTED_DIR_SEGMENTS: &[&str] = &[
    ".git", ".ssh", ".aws", ".gnupg", ".husky", ".peri", ".cc-code", ".claude", ".codex",
];

const PROTECTED_PATH_FRAGMENTS: &[&str] = &["/.github/workflows/", "/.config/gh/"];

static PROTECTED_FILE_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)^\.env(?:\..+)?$",
        r"(?i)^\.npmrc$",
        r"(?i)^\.netrc$",
        r"(?i)^\.mcp\.json$",
        r"(?i)^credentials(?:\.json)?$",
        r"(?i)^id_(?:rsa|dsa|ecdsa|ed25519)(?:\.pub)?$",
        r"(?i)\.(?:pem|key|p12|pfx)$",
        r"(?i)^\.(?:zshrc|bashrc|bash_profile|profile|zprofile|zlogin)$",
        // agent 指令文件是提示注入面
        r"(?i)^AGENTS\.md$",
        r"(?i)^CLAUDE\.md$",
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

/// 分类受保护路径。返回原因描述（`None` = 未受保护）。
pub fn protected_path_reason(path: &Path, extra: &[String]) -> Option<String> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let lowered = normalized.to_lowercase();
    let segments: Vec<&str> = lowered.split('/').filter(|s| !s.is_empty()).collect();
    let base = segments.last().copied().unwrap_or("");
    let base_original = normalized.split('/').rfind(|s| !s.is_empty()).unwrap_or("");

    for entry in extra {
        let e = entry.trim();
        if e.is_empty() {
            continue;
        }
        let le = e.to_lowercase();
        if le.contains('/') {
            if lowered.contains(&le) {
                return Some(format!("configured protected path `{e}`"));
            }
        } else if base == le {
            return Some(format!("configured protected path `{e}`"));
        }
    }

    if let Some(seg) = segments.iter().find(|s| PROTECTED_DIR_SEGMENTS.contains(s)) {
        return Some(format!("protected directory `{seg}`"));
    }
    if let Some(frag) = PROTECTED_PATH_FRAGMENTS
        .iter()
        .find(|f| lowered.contains(**f))
    {
        return Some(format!("protected path `{frag}`"));
    }
    // `.env.example` 等模板文件属于仓库，不算凭据存储
    if is_template_file(base) {
        // 仅豁免文件名校验；目录/片段校验仍然适用
        return None;
    }
    if PROTECTED_FILE_REGEXES.iter().any(|re| re.is_match(base)) {
        return Some(format!("protected file `{base_original}`"));
    }
    None
}

/// 模板/示例文件（`.env.example`、`.env.sample`、`.env.template`、`.env.dist`）。
fn is_template_file(base_lower: &str) -> bool {
    const SUFFIXES: &[&str] = &[".example", ".sample", ".template", ".dist"];
    base_lower == ".env.example"
        || SUFFIXES
            .iter()
            .any(|s| base_lower.starts_with(".env") && base_lower.ends_with(s))
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
