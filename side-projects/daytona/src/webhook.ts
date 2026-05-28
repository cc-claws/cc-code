import { Webhooks } from "@octokit/webhooks";
import type {
    EmitterWebhookEvent,
    PushEvent,
} from "@octokit/webhooks";

// ---------------------------------------------------------------------------
// 初始化 Webhooks 实例
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
// 事件处理器
// ---------------------------------------------------------------------------

webhooks.on("push", ({ id, payload }: EmitterWebhookEvent<"push">) => {
    console.log(`[webhook] push (id=${id})`);
    console.log(`  repo:   ${payload.repository.full_name}`);
    console.log(`  ref:    ${payload.ref}`);
    console.log(
        `  head:   ${payload.head_commit?.id.slice(0, 7) ?? "N/A"}`,
    );
});

webhooks.on(
    "pull_request",
    ({ id, payload }: EmitterWebhookEvent<"pull_request">) => {
        console.log(
            `[webhook] pull_request ${payload.action} (id=${id})`,
        );
        console.log(
            `  repo: ${payload.repository.full_name}`,
        );
        console.log(
            `  PR:   #${payload.pull_request.number} ${payload.pull_request.title}`,
        );
    },
);

webhooks.on(
    "issues",
    ({ id, payload }: EmitterWebhookEvent<"issues">) => {
        console.log(
            `[webhook] issues ${payload.action} (id=${id})`,
        );
        console.log(
            `  repo:  ${payload.repository.full_name}`,
        );
    },
);

webhooks.onError((error: Error) => {
    console.error(`[webhook] Error: ${error.message}`);
});
