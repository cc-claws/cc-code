// ---------------------------------------------------------------------------
// list.ts —— peri-sandbox list
// ---------------------------------------------------------------------------
import { ensureDaytonaEnv, listSandboxes } from "../daytona-helpers";
import type { Sandbox } from "@daytona/sdk";

function stateColor(state: string): string {
    switch (state) {
        case "started":  return "\x1b[32m"; // 绿
        case "error":    return "\x1b[31m"; // 红
        default:         return "\x1b[90m"; // 灰
    }
}

function icon(state: string): string {
    return state === "started" ? "\x1b[32m●\x1b[0m" : "\x1b[37m○\x1b[0m";
}

function renderRow(s: Sandbox, i: number): string {
    const state = s.state ?? "unknown";
    const c = stateColor(state);
    return `  ${String(i + 1).padStart(2)}.  ${icon(state)}  ${c}${s.name}\x1b[0m   ${c}${state}\x1b[0m   ${s.id}`;
}

export async function runList(): Promise<void> {
    ensureDaytonaEnv();

    const sandboxes = await listSandboxes();
    if (sandboxes.length === 0) {
        console.log("没有可用的 sandbox，请先运行 peri-sandbox create");
        return;
    }

    console.log(`\n已加载 ${sandboxes.length} 个沙箱:\n`);
    for (let i = 0; i < sandboxes.length; i++) {
        console.log(renderRow(sandboxes[i]!, i));
    }
    console.log();
}
