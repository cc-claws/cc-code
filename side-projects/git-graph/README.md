# gig

交互式 Git 仓库图可视化 TUI 工具。

## 安装

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install-gig.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/konghayao/peri/main/scripts/install-gig.ps1 | iex
```

### 指定版本

```bash
GIG_INSTALL_VERSION=gig-v0.1.0 curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install-gig.sh | bash
```

```powershell
$env:GIG_INSTALL_VERSION="gig-v0.1.0"; irm https://raw.githubusercontent.com/konghayao/peri/main/scripts/install-gig.ps1 | iex
```

### 环境变量

| 变量 | 说明 |
|------|------|
| `GIG_INSTALL_VERSION` | 指定版本 tag（如 `gig-v0.1.0`），留空则安装最新 |
| `PERI_INSTALL_DIR` | 安装目录（默认 `~/.peri`） |
| `GITHUB_PROXY` | GitHub 下载代理（替换 `https://github.com`） |
| `GITHUB_TOKEN` | GitHub Token（绕过 API 限流） |
| `GIG_INSTALL_PLATFORM` | 手动指定平台（如 `linux-x86_64`、`macos-aarch64`） |

## 使用

```bash
gig              # 可视化当前目录的 Git 仓库
gig /path/to/repo  # 指定仓库路径
gig update       # 自更新到最新版本
```

## 从源码构建

本项目位于 peri monorepo 的 `side-projects/git-graph`，需要克隆整个仓库：

```bash
git clone https://github.com/konghayao/peri.git
cd peri/side-projects/git-graph
cargo build --release
# 二进制位于 target/release/gig
```

## 支持平台

| 平台 | 架构 |
|------|------|
| Linux | x86_64, aarch64 |
| macOS | x86_64 (Intel), aarch64 (Apple Silicon) |
| Windows | x86_64 |

## 发布

维护者推送 `gig-v*` tag 触发 GitHub Actions 自动构建和发布：

```bash
git tag gig-v0.1.0
git push origin gig-v0.1.0
```
