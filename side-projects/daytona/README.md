# Daytona Sandbox + GitHub Webhook

这个项目做了两件事：

1. **接收 GitHub Webhook**——代码 push 了、PR 开了，自动收到通知
2. **在 Daytona 沙箱里跑 peri AI Agent**——收到 webhook 后可以让 AI 自动干活

---

## 第一步：填环境变量

把 `.env.example` 复制一份叫 `.env`，填上三个值：

```bash
DAYTONA_API_KEY=sk-xxxxxxxx      # Daytona 后台拿
DAYTONA_API_URL=https://app.daytona.io/api
GITHUB_WEBHOOK_SECRET=abc123     # 随便写一串，后面 GitHub 那边要用一样的
```

**GitHub Webhook Secret 怎么填？**

随便生成一串随机字符串就行，比如在终端跑：

```bash
openssl rand -hex 20
```

记下来，待会 GitHub 那边要填一模一样的。这个用来验签，防止别人伪造 webhook 请求。

---

## 第二步：构建

```bash
bun install
bun run build
```

产物是 `dist/app.js`，一个单文件。

---

## 第三步：部署到 Daytona

把你的项目部署到 Daytona，Daytona 会用 `dist/app.js` 里的 `fetch` 函数处理所有 HTTP 请求。

部署完成后你会得到一个域名，比如 `https://xxx.daytona.app`。

---

## 第四步：在 GitHub 上配置 Webhook

这是关键步骤，一步步来：

### 4.1 打开你的 GitHub 仓库

浏览器打开你要监听的仓库，比如 `https://github.com/你的用户名/你的仓库名`。

### 4.2 进入 Webhook 设置

页面顶部点 **Settings**（不是个人设置，是仓库的 Settings），

左侧菜单拉到最下面，点 **Webhooks**，

右上角点 **Add webhook** 按钮。

### 4.3 填写 Webhook 表单

你会看到一个表单，这样填：

| 字段 | 填什么 |
|------|--------|
| **Payload URL** | `https://你的域名/webhook`（注意结尾是 `/webhook`） |
| **Content type** | 选 `application/json` |
| **Secret** | 填第一步你在 `.env` 里写的 `GITHUB_WEBHOOK_SECRET` 那个值 |

其他字段不用改，保持默认就行。

### 4.4 选择要监听的事件

下面有两个选项：

- **"Just the push event."**——只在有人 push 代码时触发
- **"Let me select individual events."**——自己勾选，下面会展开一堆复选框

建议勾上这几个：

- [x] **Pushes**——代码推送到仓库时触发
- [x] **Pull requests**——有人开 PR、关 PR、合并时触发
- [x] **Issues**——有人开 issue、关 issue 时触发

### 4.5 保存

勾选 **Active**（默认就是勾上的），点底部绿色的 **Add webhook** 按钮。

### 4.6 验证是否生效

保存后会跳回 Webhook 列表页，你刚加的那条旁边可能有个红色感叹号，等几秒刷新。如果变成绿色对勾，说明 GitHub 已经成功连上你的服务了。

你也可以点进去，拉到最下面看 **Recent Deliveries**，随便点一条看 Response 是不是 200。

---

## 接口说明

部署后你可以直接调这些接口：

### 初始化 Sandbox

首次使用前，先调用这个接口创建沙箱环境：

```bash
curl -X PUT https://你的域名/sandbox/init
```

如果不想用默认仓库和配置，可以传参：

```bash
curl -X PUT https://你的域名/sandbox/init \
  -H "Content-Type: application/json" \
  -d '{
    "gitUrl": "https://github.com/你的用户名/你的仓库.git",
    "config": { "env": { "OPENAI_API_KEY": "sk-xxx" } }
  }'
```

### 问 peri 一个问题

```bash
curl -X POST https://你的域名/sandbox/prompt \
  -H "Content-Type: application/json" \
  -d '{ "prompt": "帮我看看 README 有什么可以改进的" }'
```

---

## 事件处理逻辑在哪里改

收到 webhook 后要做什么，改 `src/webhook.ts` 里的回调函数就行。

比如 push 了代码之后自动让 peri 跑测试：

```typescript
webhooks.on("push", ({ payload }: EmitterWebhookEvent<"push">) => {
    // 拿到刚才 push 的仓库信息
    const repo = payload.repository.full_name;
    const branch = payload.ref.replace("refs/heads/", "");

    // 在这里调用 Daytona API，或者发 HTTP 请求触发你自己的逻辑
    console.log(`${repo} 的 ${branch} 分支有新提交`);
});
```

改完记得 `bun run build` 重新构建。
