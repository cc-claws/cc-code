# Shell 安装脚本旧版本清理确认 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `install.sh` 和 `install.ps1` 安装完成后，扫描 `~/.peri/` 下的旧版本目录，列出并询问用户是否删除。

**Architecture:** 在 `main()` 函数末尾、打印 "Installation complete" 之前插入清理函数。清理函数扫描 `${INSTALL_DIR}` 匹配 `agent-v*` 的目录，排除 symlink 指向的当前版本，列出旧版本及大小，等待用户确认后删除。Bash 版本从 `/dev/tty` 读取用户输入（兼容 `curl | bash` 管道模式），PowerShell 版本用 `Read-Host`。

**Tech Stack:** Bash 3+、PowerShell 5+、标准 POSIX 工具（`du`、`readlink`/`realpath`）

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `scripts/install.sh` | 修改 | 新增 `cleanup_old_versions` 函数，在 `main()` 末尾调用 |
| `scripts/install.ps1` | 修改 | 新增 `Clean-OldVersions` 函数，在 `Main` 末尾调用 |

---

### Task 1: install.sh — 新增清理函数

**Files:**
- Modify: `scripts/install.sh`

- [ ] **Step 1: 在 `install.sh` 的 `main()` 函数之前添加 `cleanup_old_versions` 函数**

在第 88 行 `main()` 之前插入：

```bash
# --- Cleanup Old Versions ---
cleanup_old_versions() {
    local install_dir="$1"
    local current_version="$2"

    # Collect agent-v* directories, excluding current version
    local old_dirs=()
    for d in "${install_dir}"/agent-v*; do
        [[ -d "$d" ]] || continue
        local base
        base=$(basename "$d")
        [[ "$base" == "$current_version" ]] && continue
        old_dirs+=("$d")
    done

    if [[ ${#old_dirs[@]} -eq 0 ]]; then
        info "No old versions to clean up."
        return
    fi

    echo ""
    warn "Found ${#old_dirs[@]} old version(s):"
    local total_size=0
    for d in "${old_dirs[@]}"; do
        local size
        size=$(du -sh "$d" 2>/dev/null | cut -f1)
        echo "  $(basename "$d")  (${size})"
        local size_bytes
        size_bytes=$(du -s "$d" 2>/dev/null | cut -f1)
        total_size=$((total_size + size_bytes))
    done
    local total_human
    total_human=$(du -sh "${old_dirs[@]}" 2>/dev/null | tail -1 | cut -f1)
    echo "  Total: ${total_human}"
    echo ""

    # Read from /dev/tty to work with curl | bash pipe
    if ! [[ -t 0 ]] && [[ -e /dev/tty ]]; then
        exec 3< /dev/tty
    else
        exec 3<&0
    fi

    echo -e "${YELLOW}[WARN]${NC}  Delete old versions? [y/N] " >&2
    local answer
    read -r answer <&3
    exec 3<&-

    case "${answer}" in
        [yY]|[yY][eE][sS])
            for d in "${old_dirs[@]}"; do
                rm -rf "$d"
                info "Removed: $(basename "$d")"
            done
            info "Cleaned up ${#old_dirs[@]} old version(s)."
            ;;
        *)
            info "Skipped cleanup."
            ;;
    esac
}
```

- [ ] **Step 2: 在 `main()` 中调用清理函数**

在 `install.sh` 第 222 行（`echo ""` 和 `info "Installation complete! Version: ${VERSION_TAG}"` 之间）插入调用。找到这段：

```bash
    echo ""
    info "Installation complete! Version: ${VERSION_TAG}"
```

改为：

```bash
    # Offer to clean up old versions
    cleanup_old_versions "${INSTALL_DIR}" "${VERSION_TAG}"

    echo ""
    info "Installation complete! Version: ${VERSION_TAG}"
```

- [ ] **Step 3: 手动验证**

```bash
# 模拟：创建几个假版本目录
mkdir -p ~/.peri/agent-v0.1-test ~/.peri/agent-v0.2-test
echo "fake" > ~/.peri/agent-v0.1-test/peri
echo "fake" > ~/.peri/agent-v0.2-test/peri

# 运行安装脚本（会下载真实版本，安装完成后应提示清理）
# 由于无法自动化交互测试，手动运行验证：
PERI_INSTALL_VERSION=agent-v0.99.9 bash scripts/install.sh
# 预期：安装完成后列出旧版本并询问是否删除
# 按 y 确认删除，按 n 或回车跳过

# 清理测试目录
rm -rf ~/.peri/agent-v0.1-test ~/.peri/agent-v0.2-test
```

- [ ] **Step 4: 验证管道模式兼容**

```bash
# 模拟 curl | bash 管道模式
echo 'source scripts/install.sh' | PERI_INSTALL_VERSION=agent-v0.99.9 bash
# 预期：安装完成后仍能提示清理（从 /dev/tty 读取）
# 如果没有 /dev/tty（如 CI 环境），应跳过清理不卡住
```

- [ ] **Step 5: Commit**

```bash
git add scripts/install.sh
git commit -m "feat(install): add old version cleanup with user confirmation"
```

---

### Task 2: install.ps1 — 新增清理函数

**Files:**
- Modify: `scripts/install.ps1`

- [ ] **Step 1: 在 `install.ps1` 的 `Main` 函数之前添加 `Clean-OldVersions` 函数**

在第 71 行 `function Main {` 之前插入：

```powershell
# --- Cleanup Old Versions ---
function Clean-OldVersions {
    param([string]$InstallDir, [string]$CurrentVersion)

    # Collect agent-v* directories, excluding current version
    $oldDirs = @()
    Get-ChildItem -Path $InstallDir -Directory | Where-Object {
        $_.Name -match '^agent-v' -and $_.Name -ne $CurrentVersion
    } | ForEach-Object {
        $oldDirs += $_
    }

    if ($oldDirs.Count -eq 0) {
        info "No old versions to clean up."
        return
    }

    Write-Host ""
    warn "Found $($oldDirs.Count) old version(s):"
    $totalSize = 0
    foreach ($d in $oldDirs) {
        $size = (Get-ChildItem -Path $d.FullName -Recurse -File -ErrorAction SilentlyContinue |
                 Measure-Object -Property Length -Sum).Sum
        $totalSize += $size
        $sizeMB = [math]::Round($size / 1MB, 1)
        Write-Host "  $($d.Name)  ($sizeMB MB)"
    }
    $totalMB = [math]::Round($totalSize / 1MB, 1)
    Write-Host "  Total: $totalMB MB"
    Write-Host ""

    $answer = Read-Host "Delete old versions? [y/N]"
    switch ($answer) {
        { $_ -match '^[yY](es)?$' } {
            foreach ($d in $oldDirs) {
                Remove-Item -Recurse -Force $d.FullName
                info "Removed: $($d.Name)"
            }
            info "Cleaned up $($oldDirs.Count) old version(s)."
        }
        default {
            info "Skipped cleanup."
        }
    }
}
```

- [ ] **Step 2: 在 `Main` 中调用清理函数**

在 `install.ps1` 第 208 行（`Write-Host ""` 和 `info "Installation complete! Version: $VersionTag"` 之间）插入调用。找到这段：

```powershell
    Write-Host ""
    info "Installation complete! Version: $VersionTag"
```

改为：

```powershell
    # Offer to clean up old versions
    Clean-OldVersions -InstallDir $InstallDir -CurrentVersion $VersionTag

    Write-Host ""
    info "Installation complete! Version: $VersionTag"
```

- [ ] **Step 3: 手动验证（Windows）**

```powershell
# 创建测试目录
New-Item -ItemType Directory -Force "$env:USERPROFILE\.peri\agent-v0.1-test"
New-Item -ItemType Directory -Force "$env:USERPROFILE\.peri\agent-v0.2-test"

# 运行安装脚本
$env:PERI_INSTALL_VERSION = "agent-v0.99.9"
irm https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.ps1 | iex
# 预期：列出旧版本，等待 y/N 确认

# 清理测试目录
Remove-Item -Recurse -Force "$env:USERPROFILE\.peri\agent-v0.1-test"
Remove-Item -Recurse -Force "$env:USERPROFILE\.peri\agent-v0.2-test"
```

- [ ] **Step 4: Commit**

```bash
git add scripts/install.ps1
git commit -m "feat(install): add old version cleanup with user confirmation (Windows)"
```

---

## Self-Review

**1. Spec 覆盖度：**
- ✅ 安装完成后扫描旧版本 → Task 1 Step 1、Task 2 Step 1
- ✅ 排除当前版本 → `[[ "$base" == "$current_version" ]]` / `-ne $CurrentVersion`
- ✅ 列出旧版本及大小 → `du -sh` / `Measure-Object`
- ✅ 用户确认后删除 → `read -r` / `Read-Host`
- ✅ 用户可跳过 → 默认 N

**2. Placeholder 扫描：** 无 TBD/TODO，所有代码完整。

**3. 类型一致性：** 两个脚本独立，无跨脚本类型依赖。

**4. 边界情况：**
- 无旧版本 → `old_dirs` 为空，打印 "No old versions" 直接返回
- 管道模式（`curl | bash`）→ 从 `/dev/tty` 读取，无 tty 时 `exec 3<&0` 回退到 stdin
- PowerShell 管道 → `Read-Host` 本身会弹出提示，无管道兼容问题
