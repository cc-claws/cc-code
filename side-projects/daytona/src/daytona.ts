import { Daytona } from "@daytona/sdk";
import type { Sandbox } from "@daytona/sdk";
import fs from "node:fs";

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------
const SANDBOX_NAME = "Perihelion Sandbox";
const MOUNT_DIR = "/home/daytona/code";
const GIT_URL = "https://github.com/KonghaYao/peri.git";
const DEFAULT_PERI_CONFIG_PATH = "./settings.json";

// ---------------------------------------------------------------------------
// 类型
// ---------------------------------------------------------------------------
interface CommandResult {
    exitCode: number;
    result: string;
}

export type PeriConfig = Record<string, unknown>;

// ---------------------------------------------------------------------------
// 全局单例
// ---------------------------------------------------------------------------
const daytona = new Daytona();
const periConfig: PeriConfig = JSON.parse(
    fs.readFileSync(DEFAULT_PERI_CONFIG_PATH, "utf-8"),
);

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------
export function shellEscape(value: string): string {
    return "'" + value.replace(/'/g, "'\\''") + "'";
}

// ---------------------------------------------------------------------------
// Sandbox 操作
// ---------------------------------------------------------------------------

/** 在 sandbox 内按顺序执行 shell 命令 */
export async function executeCommandList(
    sandbox: Sandbox,
    commands: string[],
    cwd: string,
): Promise<CommandResult[]> {
    if (commands.length === 0) return [];
    const results: CommandResult[] = [];
    for (const command of commands) {
        const response = await sandbox.process.executeCommand(
            command,
            cwd,
            undefined,
            120,
        );
        console.log(
            `[daytona] cmd: ${command}\n  exit=${response.exitCode} out=${response.result.slice(0, 200)}`,
        );
        if (response.exitCode !== 0) {
            throw new Error(
                `Command failed (exit ${response.exitCode}): ${command}\n${response.result}`,
            );
        }
        results.push(response);
    }
    return results;
}

/** 初始化 sandbox：创建 sandbox → clone 仓库 → 安装 peri CLI → 写入配置 */
export async function initSandbox(
    gitUrl: string,
    config: PeriConfig,
): Promise<void> {
    console.log("[daytona] Step 1/3: Creating sandbox...");
    const sandbox = await daytona.create({
        name: SANDBOX_NAME,
        language: "typescript",
    });
    console.log(`[daytona] Sandbox created: ${sandbox.id}`);

    console.log(`[daytona] Step 2/3: Cloning ${gitUrl} → ${MOUNT_DIR}...`);
    await sandbox.git.clone(gitUrl, MOUNT_DIR, "main");

    console.log("[daytona] Step 3/3: Installing peri CLI + writing config...");
    await executeCommandList(
        sandbox,
        [
            "curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash",
            `mkdir -p ${MOUNT_DIR}/.peri && cat <<'EOF' > ${MOUNT_DIR}/.peri/settings.json\n${JSON.stringify(config, null, 2)}\nEOF`,
        ],
        MOUNT_DIR,
    );
    console.log("[daytona] Sandbox initialized successfully");
}

/** 向 peri AI Agent 发送单轮问答（print 模式） */
export async function askPeri(inputPrompt: string): Promise<string> {
    const sandbox = await daytona.get(SANDBOX_NAME);
    console.log(
        `[daytona] Sandbox: ${sandbox.id} (${sandbox.state})`,
    );
    if (sandbox.state === "stopped") {
        await sandbox.start();
        console.log(`[daytona] Sandbox started: ${sandbox.id}`);
    }
    const results = await executeCommandList(
        sandbox,
        [`/home/daytona/.peri/peri -p ${shellEscape(inputPrompt)}`],
        MOUNT_DIR,
    );
    return results[0]!.result;
}
