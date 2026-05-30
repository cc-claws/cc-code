# peri-sandbox

在 [Daytona](https://daytona.io) 沙箱中运行 peri AI Agent。

两部分：

- **CLI 工具** — `init` / `create` / `ask` 三组命令，本地终端操作沙箱
- **Server** — 接收 GitHub Webhook，自动触发 agent 处理 Issue/PR

---

## 安装

```bash
bun install
```

---

## CLI 使用

首次使用，先配置 Daytona 连接：

```bash
bun run src/cli.ts init
```

```text
? Daytona API Key     ****
? Daytona API URL     https://app.daytona.io/api
? 确认保存? (Y/n)

→ 配置保存到 ~/.peri-sandbox/config.json
```

然后创建沙箱：

```bash
bun run src/cli.ts create
```

```text
? 选择快照（回车跳过则使用默认）   [搜索过滤 daytona-* 快照]
? 沙箱名称                        Perihelion Sandbox
? Git 仓库地址                     https://github.com/KonghaYao/peri.git
? peri 配置文件（本地路径，将传输到沙箱内）  ./settings.json

即将执行:
  沙箱名称:                  Perihelion Sandbox
  Git 仓库:                  https://github.com/...
  peri 配置（本地传沙箱）:    ./settings.json
  快照:                      daytona-typescript

? 确认创建? (Y/n)
```

### 向 peri 提问

```bash
bun run src/cli.ts ask "帮我看看 README 有什么可以改进的"
bun run src/cli.ts ask --sandbox my-sandbox "修复所有 clippy 警告"
```

不指定 `--sandbox` 时会用 ↑↓ 选择沙箱，状态用颜色区分：

```text
? 选择沙箱
    ●  Perihelion Sandbox           ← 绿（在线）
    ○  my-box                       ← 灰（离线）
    ○  broken-box                   ← 红（错误）
```

### 构建单文件

```bash
bun run build:cli
# 产物: dist/peri-sandbox
```

```bash
./dist/peri-sandbox init
./dist/peri-sandbox create
./dist/peri-sandbox ask "你的问题"
```

---

## 环境变量

CLI 配置通过 `bun init` 存入 `~/.peri-sandbox/config.json`，无需手动设环境变量。
Server 需要以下环境变量（.env）：

| 变量 | 说明 |
|------|------|
| `DAYTONA_API_KEY` | Daytona API Key（仅 server 必需） |
| `DAYTONA_API_URL` | Daytona API 地址（默认 `https://app.daytona.io/api`） |
| `GITHUB_WEBHOOK_SECRET` | GitHub Webhook 验签密钥（仅 server 需要） |

---

## Server（可选）

构建并部署到 Daytona：

```bash
bun run build:server
# 产物: dist/app.js
```

接口：

```bash
# 健康检查
curl https://你的域名/health

# 初始化沙箱
curl -X PUT https://你的域名/sandbox/init
curl -X PUT https://你的域名/sandbox/init \
  -H "Content-Type: application/json" \
  -d '{"gitUrl": "https://github.com/you/repo.git", "config": {}}'

# 向 peri 提问
curl -X POST https://你的域名/sandbox/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt": "帮我看看 README 有什么可以改进的"}'
```
