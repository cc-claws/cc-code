// ---------------------------------------------------------------------------
// init.ts —— peri-sandbox init（初始化 Daytona 连接）
// ---------------------------------------------------------------------------
import { input, password, confirm } from "@inquirer/prompts";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { loadDaytonaConfig } from "../daytona-helpers";

export interface InitParams {
    apiKey?: string;
    apiUrl?: string;
}

function configDir(): string {
    return path.join(os.homedir(), ".peri-sandbox");
}

function configPath(): string {
    return path.join(configDir(), "config.json");
}

export async function runInit(params?: InitParams): Promise<void> {
    console.log("\n初始化 Daytona 连接\n");

    let apiKey: string;
    let apiUrl: string;

    if (params) {
        // 非交互模式
        apiKey = params.apiKey ?? process.env.DAYTONA_API_KEY ?? "";
        apiUrl = params.apiUrl ?? loadDaytonaConfig().apiUrl;
        if (!apiKey) {
            console.error("[错误] --params 中缺少 apiKey");
            process.exit(1);
        }
    } else {
        // 交互模式
        const { apiUrl: defaultUrl } = loadDaytonaConfig();
        apiKey = await password({ message: "Daytona API Key", mask: "*" });
        apiUrl = await input({
            message: "Daytona API URL",
            default: defaultUrl || "https://app.daytona.io/api",
        });
        if (!apiKey) {
            console.error("\nAPI Key 不能为空");
            process.exit(1);
        }
        console.log("\n即将保存:\n");
        console.log(`  API Key    ****${apiKey.slice(-4)}`);
        console.log(`  API URL    ${apiUrl}`);
        console.log(`  保存位置    ${configPath()}`);

        const ok = await confirm({ message: "\n确认保存?", default: true });
        if (!ok) {
            console.log("已取消。");
            return;
        }
    }

    fs.mkdirSync(configDir(), { recursive: true });
    fs.writeFileSync(configPath(), JSON.stringify({ apiKey, apiUrl }, null, 2), "utf-8");
    console.log(`\n已保存到 ${configPath()}`);
    console.log(`现在可以运行: peri-sandbox create`);
}
