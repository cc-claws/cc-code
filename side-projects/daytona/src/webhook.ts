import { Webhooks } from "@octokit/webhooks";
import type { EmitterWebhookEvent } from "@octokit/webhooks";
import { dispatcher } from "./dispatch";

// ---------------------------------------------------------------------------
// 初始化
// ---------------------------------------------------------------------------
const WEBHOOK_SECRET = process.env.GITHUB_WEBHOOK_SECRET;

if (!WEBHOOK_SECRET) {
    console.warn(
        "[webhook] GITHUB_WEBHOOK_SECRET not set — signature verification disabled",
    );
}

export const webhooks = new Webhooks({
    secret: WEBHOOK_SECRET || "no-secret-set",
});

// ---------------------------------------------------------------------------
// 事件 → 派发器桥接
// ---------------------------------------------------------------------------

/**
 * 将 GitHub 的扁平事件名（如 "pull_request"）和 payload.action
 * 拼成调度事件名（如 "pull_request.opened"），然后交给 dispatcher。
 */
function forward(event: EmitterWebhookEvent<any>): void {
    const type = "action" in event.payload
        ? `${event.name}.${(event.payload as any).action}`
        : event.name;

    console.log(`[webhook] ${type} (id=${event.id})`);
    dispatcher.dispatch(type, event.payload);
}

webhooks.on("issues", forward);
webhooks.on("pull_request", forward);

webhooks.onError((error: Error) => {
    console.error(`[webhook] Error: ${error.message}`);
});
