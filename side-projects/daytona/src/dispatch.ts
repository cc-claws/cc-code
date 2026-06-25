// ---------------------------------------------------------------------------
// 事件派发器 —— webhook 事件 → 业务逻辑的解耦桥接
//
// 用法：
//   import { dispatcher } from "./dispatch";
//   dispatcher.on("push", async (e) => { ... });
//   dispatcher.on("pull_request.*", async (e) => { ... });
//   await dispatcher.dispatch("push", payload);
// ---------------------------------------------------------------------------

/** 派发器收到的标准化事件 */
export interface WebhookEvent {
    /** 事件名，如 "push"、"pull_request.opened"、"issues.closed" */
    type: string;
    /** 原始 payload（取决于 GitHub 事件类型） */
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    payload: any;
}

type EventHandler = (event: WebhookEvent) => Promise<void>;

// ---------------------------------------------------------------------------
// 内部：pattern 匹配
// ---------------------------------------------------------------------------

/**
 * 判断事件 type 是否匹配注册 pattern。
 *
 * "push"              → 精确匹配 "push"
 * "pull_request.*"    → 匹配 "pull_request.opened"、"pull_request.closed" 等
 */
function matchPattern(pattern: string, eventType: string): boolean {
    if (pattern === eventType) return true;
    if (pattern.endsWith(".*")) {
        const prefix = pattern.slice(0, -2); // "pull_request."
        return eventType.startsWith(prefix);
    }
    return false;
}

// ---------------------------------------------------------------------------
// 单例
// ---------------------------------------------------------------------------

class EventDispatcher {
    private handlers: { pattern: string; handler: EventHandler }[] = [];

    /** 注册一个事件处理器 */
    on(pattern: string, handler: EventHandler): void {
        this.handlers.push({ pattern, handler });
    }

    /** 派发事件，触发所有匹配的处理器（并发执行） */
    async dispatch(type: string, payload: unknown): Promise<void> {
        const matched = this.handlers.filter(({ pattern }) =>
            matchPattern(pattern, type),
        );
        if (matched.length === 0) {
            console.log(`[dispatch] no handler for "${type}"`);
            return;
        }
        console.log(
            `[dispatch] "${type}" → ${matched.length} handler(s)`,
        );
        await Promise.all(
            matched.map(({ handler }) => handler({ type, payload })),
        );
    }
}

/** 全局单例 —— 整个应用共享一个派发器 */
export const dispatcher = new EventDispatcher();
