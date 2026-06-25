// ---------------------------------------------------------------------------
// ask.ts —— peri-sandbox ask [--sandbox NAME] <prompt>
// ---------------------------------------------------------------------------
import { select } from "@inquirer/prompts";
import { Daytona } from "@daytona/sdk";
import type { Sandbox } from "@daytona/sdk";
import ora from "ora";
import {
    MOUNT_DIR,
    PERI_BIN,
    listSandboxes,
    findSandbox,
    ensureDaytonaEnv,
} from "../daytona-helpers";

export interface AskOptions {
    sandbox?: string;
    prompt?: string;
}

function sandboxChoice(s: Sandbox) {
    const state = s.state ?? "unknown";
    const online = state === "started";
    const icon = online ? "\x1b[32m●\x1b[0m" : "\x1b[37m○\x1b[0m";
    const c = online ? "\x1b[32m"    // 绿
          : state === "error" ? "\x1b[31m"  // 红
          : "\x1b[90m";                      // 灰
    return {
        name: `  ${icon}  ${c}${s.name}\x1b[0m`,
        value: s,
        description: s.id,
    };
}

export async function askPeri(opts: AskOptions): Promise<void> {
    ensureDaytonaEnv();

    const prompt = opts.prompt;
    if (!prompt || prompt.trim().length === 0) {
        console.error("用法: peri-sandbox ask <prompt>");
        console.error("示例: peri-sandbox ask \"帮我看看 README 有什么可以改进的\"");
        process.exit(1);
    }

    // 获取 sandbox
    let sandbox: Sandbox;
    if (opts.sandbox) {
        const found = await findSandbox(opts.sandbox);
        if (!found) {
            console.error(`未找到 sandbox: ${opts.sandbox}`);
            process.exit(1);
        }
        sandbox = found;
    } else {
        const sandboxes = await listSandboxes();
        if (sandboxes.length === 0) {
            console.error("没有可用的 sandbox，请先运行 peri-sandbox create");
            process.exit(1);
        }
        sandbox = await select({
            message: "选择沙箱",
            choices: sandboxes.map(sandboxChoice),
        });
    }

    // 确保运行状态
    if (sandbox.state === "stopped") {
        const sp = ora(`启动 ${sandbox.name}...`).start();
        try {
            await sandbox.start();
            sp.succeed(`${sandbox.name} 已启动`);
        } catch (err) {
            sp.fail(`启动失败: ${err instanceof Error ? err.message : err}`);
            process.exit(1);
        }
    }

    const online = sandbox.state === "started" || sandbox.state === undefined; // 启动后状态可能未刷新
    const c = online ? "\x1b[32m" : "\x1b[90m";
    process.stdout.write(`[ask] ${c}${sandbox.name}\x1b[0m ${online ? "\x1b[32m●\x1b[0m" : "\x1b[37m○\x1b[0m"}\n`);

    // 转义单引号防止 shell 注入
    const escaped = prompt.replace(/'/g, "'\\''");
    const cmd = `${PERI_BIN} -p '${escaped}'`;

    const sp = ora({
        text: `peri -p "${prompt.slice(0, 50)}${prompt.length > 50 ? "..." : ""}"`,
        prefixText: "⏳",
    }).start();

    const result = await sandbox.process.executeCommand(
        cmd,
        MOUNT_DIR,
        undefined,
        300,
    );

    sp.stop();
    console.log("─".repeat(60));
    console.log(result.result);

    if (result.exitCode !== 0) {
        console.error(`\n[ask] peri 退出码: ${result.exitCode}`);
        process.exit(1);
    } else {
        console.log("\x1b[32m✓\x1b[0m 完成");
    }
}
