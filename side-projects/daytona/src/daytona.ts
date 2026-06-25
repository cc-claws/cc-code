import { Daytona } from "@daytona/sdk";
import {
    SANDBOX_NAME,
    MOUNT_DIR,
    PERI_BIN,
    executeCommandList,
    ensureRunning,
} from "./daytona-helpers";

export type PeriConfig = Record<string, unknown>;

const daytona = new Daytona();

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------
export function shellEscape(value: string): string {
    return "'" + value.replace(/'/g, "'\\''") + "'";
}

// ---------------------------------------------------------------------------
// Sandbox 操作
// ---------------------------------------------------------------------------

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
        { cwd: MOUNT_DIR },
    );
    console.log("[daytona] Sandbox initialized successfully");
}

/** 向 peri AI Agent 发送单轮问答（print 模式） */
export async function askPeri(inputPrompt: string): Promise<string> {
    const sandbox = await daytona.get(SANDBOX_NAME);
    console.log(
        `[daytona] Sandbox: ${sandbox.id} (${sandbox.state})`,
    );
    await ensureRunning(sandbox);

    const { result } = await sandbox.process.executeCommand(
        `${PERI_BIN} -p ${shellEscape(inputPrompt)}`,
        MOUNT_DIR,
        undefined,
        300,
    );
    return result;
}
