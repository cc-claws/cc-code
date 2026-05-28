import { Hono } from "hono";
import { webhooks } from "./webhook";
import { initSandbox, askPeri } from "./daytona";

// ---------------------------------------------------------------------------
// Hono 应用
// ---------------------------------------------------------------------------
const app = new Hono();

// ---------------------------------------------------------------------------
// 健康检查
// ---------------------------------------------------------------------------
app.get("/", (c) => c.text("Hello, World!"));
app.get("/health", (c) => c.json({ status: "ok" }));

// ---------------------------------------------------------------------------
// GitHub Webhook 接收
// ---------------------------------------------------------------------------
app.post("/webhook", async (c) => {
    const id = c.req.header("x-github-delivery") || "";
    const name = c.req.header("x-github-event") || "unknown";
    const signature = c.req.header("x-hub-signature-256") || "";

    try {
        const body = await c.req.json();
        const event = await webhooks.verifyAndReceive({
            id,
            name: name as any,
            signature,
            payload: JSON.stringify(body),
        });
        return c.json({ ok: true, event: event.name });
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        const status =
            message.includes("signature") || message.includes("secret")
                ? 401
                : 400;
        return c.json({ ok: false, error: message }, status);
    }
});

// ---------------------------------------------------------------------------
// Sandbox 操作
// ---------------------------------------------------------------------------

/** POST /sandbox/prompt —— 向 peri 发送问答 */
app.post("/sandbox/prompt", async (c) => {
    const { prompt } = await c.req.json<{ prompt?: string }>();
    if (!prompt) {
        return c.json({ error: "Missing 'prompt' field" }, 400);
    }
    try {
        const result = await askPeri(prompt);
        return c.text(result);
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return c.json({ error: message }, 500);
    }
});

/** PUT /sandbox/init —— 初始化 sandbox 环境 */
app.put("/sandbox/init", async (c) => {
    try {
        // 从请求体中可选覆盖 gitUrl / config
        const body = await c.req.json().catch(() => ({}));
        const gitUrl = body.gitUrl ?? "https://github.com/KonghaYao/peri.git";
        const config = body.config ?? {}; // 如果未提供，使用 daytona.ts 中的默认配置
        await initSandbox(gitUrl, config);
        return c.json({ ok: true, message: "Sandbox initialized" });
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return c.json({ error: message }, 500);
    }
});

// ---------------------------------------------------------------------------
// Daytona / Bun 入口
// ---------------------------------------------------------------------------
export default {
    fetch: app.fetch,
};
