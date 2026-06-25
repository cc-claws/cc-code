// ---------------------------------------------------------------------------
// daytona-helpers.ts —— 共享的 Daytona 操作
// ---------------------------------------------------------------------------
import { Daytona } from "@daytona/sdk";
import type { Sandbox } from "@daytona/sdk";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------
export const SANDBOX_NAME = "Perihelion Sandbox";
export const MOUNT_DIR = "/home/daytona/code";
export const PERI_BIN = "/home/daytona/.peri/peri";

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------

/** 在 sandbox 内按顺序执行 shell 命令，任一失败抛异常 */
export async function executeCommandList(
    sandbox: Sandbox,
    commands: string[],
    opts?: { cwd?: string; spinner?: { text: string; suffixText: string } },
): Promise<void> {
    const cwd = opts?.cwd ?? MOUNT_DIR;
    for (const command of commands) {
        if (opts?.spinner) {
            process.stdout.write(`\x1b[2K\r${opts.spinner.text} (${opts.spinner.suffixText})\r`);
        }
        const { exitCode, result } = await sandbox.process.executeCommand(
            command,
            cwd,
            undefined,
            120,
        );
        if (exitCode !== 0) {
            throw new Error(
                `Command failed (exit ${exitCode}): ${command}\n${result}`,
            );
        }
        if (!opts?.spinner) {
            const preview = result.slice(0, 200).replace(/\n/g, "\\n");
            console.log(`  → ${command.slice(0, 80)}... exit=${exitCode}  ${preview}`);
        }
    }
}

/** 列出所有 sandbox */
export async function listSandboxes(): Promise<Sandbox[]> {
    const daytona = new Daytona();
    const result: Sandbox[] = [];
    for await (const sb of daytona.list()) {
        result.push(sb);
    }
    return result;
}

/** 按名称或 ID 查找 sandbox */
export async function findSandbox(nameOrId: string): Promise<Sandbox | null> {
    const daytona = new Daytona();
    for await (const sb of daytona.list({ name: nameOrId })) {
        if (sb.name === nameOrId || sb.id === nameOrId) return sb;
    }
    return null;
}

/** 确保 sandbox 处于运行状态 */
export async function ensureRunning(sandbox: Sandbox): Promise<void> {
    if (sandbox.state === "stopped") {
        console.log(`[sandbox] 启动 ${sandbox.name} ...`);
        await sandbox.start();
    }
}

// ---------------------------------------------------------------------------
// Daytona 连接配置
// ---------------------------------------------------------------------------

export interface DaytonaConfig {
    apiKey: string;
    apiUrl: string;
}

function daytonaConfigDir(): string {
    return path.join(os.homedir(), ".peri-sandbox");
}

function daytonaConfigPath(): string {
    return path.join(daytonaConfigDir(), "config.json");
}

/** 读取 Daytona 连接配置（不修改 process.env） */
export function loadDaytonaConfig(): DaytonaConfig {
    const cfgPath = daytonaConfigPath();
    if (fs.existsSync(cfgPath)) {
        try {
            const raw = JSON.parse(fs.readFileSync(cfgPath, "utf-8"));
            return {
                apiKey: raw.apiKey || "",
                apiUrl: raw.apiUrl || "https://app.daytona.io/api",
            };
        } catch {
            // JSON 损坏，回退
        }
    }
    return {
        apiKey: process.env.DAYTONA_API_KEY ?? "",
        apiUrl: process.env.DAYTONA_API_URL ?? "https://app.daytona.io/api",
    };
}

/** 确保 DAYTONA_API_KEY / DAYTONA_API_URL 环境变量已设置 */
export function ensureDaytonaEnv(): void {
    // 环境变量优先
    if (!process.env.DAYTONA_API_KEY || !process.env.DAYTONA_API_URL) {
        const cfg = loadDaytonaConfig();
        if (!process.env.DAYTONA_API_KEY && cfg.apiKey) {
            process.env.DAYTONA_API_KEY = cfg.apiKey;
        }
        if (!process.env.DAYTONA_API_URL && cfg.apiUrl) {
            process.env.DAYTONA_API_URL = cfg.apiUrl;
        }
    }
    if (!process.env.DAYTONA_API_KEY) {
        console.error("[错误] 未设置 DAYTONA_API_KEY，请先运行 peri-sandbox init");
        process.exit(1);
    }
}

// ---------------------------------------------------------------------------
// 配置读取
// ---------------------------------------------------------------------------

/** 读取 peri settings.json */
export function loadConfig(configPath: string): Record<string, unknown> {
    if (fs.existsSync(configPath)) {
        console.log(`[配置] 已加载 ${configPath}`);
        return JSON.parse(fs.readFileSync(configPath, "utf-8"));
    }
    console.log(`[配置] 未找到 ${configPath}，使用空配置`);
    return {};
}

// ---------------------------------------------------------------------------
// --params 解析
// ---------------------------------------------------------------------------

/** 解析 --params JSON 字符串，提供后跳过交互式填表 */
export function parseParamsArg(raw: string): Record<string, string> {
    try {
        const obj = JSON.parse(raw);
        if (typeof obj !== "object" || obj === null) {
            throw new Error("params 必须是 JSON 对象");
        }
        return obj as Record<string, string>;
    } catch (err) {
        console.error(`--params JSON 解析失败: ${err instanceof Error ? err.message : err}`);
        process.exit(1);
    }
}
