import { Hono } from "hono";
import { webhooks } from "./webhook";
import { initSandbox, askPeri } from "./daytona";
import { dispatcher } from "./dispatch";

// ---------------------------------------------------------------------------
// 事件 → 业务逻辑映射
// ---------------------------------------------------------------------------

// Issue 被打上 "ai-solve" 标签 → 触发 agent 处理
dispatcher.on("issues.labeled", async ({ payload }) => {
    const p = payload as any;
    if (p.label?.name !== "ai-solve") return;

    const issue = p.issue;
    const repo = p.repository;
    const sender = p.sender?.login ?? "unknown";

    console.log(`[event] issue #${issue.number} tagged ai-solve → agent`);

    await askPeri(
        [
            `GitHub Issue #${issue.number} tagged "ai-solve" by @${sender}.`,
            "",
            `Repository: ${repo.full_name} (${repo.html_url})`,
            `Issue URL:   ${issue.html_url}`,
            `Title:       ${issue.title}`,
            `State:       ${issue.state}`,
            `Author:      @${issue.user.login}`,
            issue.assignee ? `Assignee:    @${issue.assignee.login}` : null,
            issue.milestone ? `Milestone:   ${issue.milestone.title}` : null,
            issue.labels?.length
                ? `Labels:      ${issue.labels.map((l: any) => l.name).join(", ")}`
                : null,
            `Created:     ${issue.created_at}`,
            "",
            "---",
            "",
            issue.body ?? "(no description)",
            "",
            "---",
            "",
            `You have access to the \`gh\` CLI to interact with this issue.`,
            `To post a comment: gh issue comment ${issue.number} --repo ${repo.full_name} --body "...your message..."`,
            `To close this issue: gh issue close ${issue.number} --repo ${repo.full_name}`,
        ]
            .filter(Boolean)
            .join("\n"),
    );
});

// PR 事件（opened / synchronize / reopened 等）→ 自动触发 agent
dispatcher.on("pull_request.opened", async ({ payload }) => {
    const p = payload as any;
    const pr = p.pull_request;
    const repo = p.repository;

    console.log(`[event] PR #${pr.number} ${p.action} → agent`);

    await askPeri(
        [
            `GitHub PR #${pr.number} ${p.action} by @${pr.user.login}.`,
            "",
            `Repository: ${repo.full_name} (${repo.html_url})`,
            `PR URL:     ${pr.html_url}`,
            `Diff URL:   ${pr.diff_url}`,
            `Title:      ${pr.title}`,
            `Branch:     ${pr.head.ref} → ${pr.base.ref}`,
            `State:      ${pr.state}${pr.draft ? " (draft)" : ""}${pr.merged ? " (merged)" : ""}`,
            pr.assignee ? `Assignee:   @${pr.assignee.login}` : null,
            `Created:    ${pr.created_at}`,
            pr.updated_at !== pr.created_at ? `Updated:    ${pr.updated_at}` : null,
            "",
            "---",
            "",
            pr.body ?? "(no description)",
            "",
            "---",
            "",
            `You have access to the \`gh\` CLI to interact with this PR.`,
            `Check out the PR branch: gh pr checkout ${pr.number}`,
            `Post a review comment: gh pr comment ${pr.number} --repo ${repo.full_name} --body "...your review..."`,
            `Approve the PR: gh pr review ${pr.number} --repo ${repo.full_name} --approve`,
            `Request changes: gh pr review ${pr.number} --repo ${repo.full_name} --request-changes --body "...reason..."`,
        ]
            .filter(Boolean)
            .join("\n"),
    );
});

// ---------------------------------------------------------------------------
// Hono 应用
// ---------------------------------------------------------------------------
const app = new Hono();

// 健康检查
app.get("/", (c) => c.text("Hello, World!"));
app.get("/health", (c) => c.json({ status: "ok" }));

// GitHub Webhook 接收
app.post("/webhook", async (c) => {
    const id = c.req.header("x-github-delivery") || "";
    const name = c.req.header("x-github-event") || "unknown";
    const signature = c.req.header("x-hub-signature-256") || "";

    try {
        const body = await c.req.json();
        await webhooks.verifyAndReceive({
            id,
            name: name as any,
            signature,
            payload: JSON.stringify(body),
        });
        return c.json({ ok: true, event: name });
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        const status =
            message.includes("signature") || message.includes("secret")
                ? 401
                : 400;
        return c.json({ ok: false, error: message }, status);
    }
});

// Sandbox 操作
app.post("/sandbox/prompt", async (c) => {
    const { prompt } = await c.req.json<{ prompt?: string }>();
    if (!prompt) return c.json({ error: "Missing 'prompt' field" }, 400);
    try {
        const result = await askPeri(prompt);
        return c.text(result);
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return c.json({ error: message }, 500);
    }
});

app.put("/sandbox/init", async (c) => {
    try {
        const body = await c.req.json().catch(() => ({}));
        await initSandbox(
            body.gitUrl ?? "https://github.com/KonghaYao/peri.git",
            body.config ?? {},
        );
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
