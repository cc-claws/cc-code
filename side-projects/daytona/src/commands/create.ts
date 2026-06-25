// ---------------------------------------------------------------------------
// create.ts —— peri-sandbox create（交互式填表创建沙箱）
// ---------------------------------------------------------------------------
import { input, confirm, search } from "@inquirer/prompts";
import { Daytona } from "@daytona/sdk";
import ora from "ora";
import {
    MOUNT_DIR,
    executeCommandList,
    loadConfig,
    findSandbox,
    ensureDaytonaEnv,
} from "../daytona-helpers";

export interface CreateParams {
    name?: string;
    gitUrl?: string;
    snapshot?: string;
    config?: string;
}

const DEFAULT_NAME = "Perihelion Sandbox";
const DEFAULT_GIT_URL = "https://github.com/KonghaYao/peri.git";
const DEFAULT_CONFIG_PATH = "./settings.json";

export async function runCreate(params?: CreateParams): Promise<void> {
    ensureDaytonaEnv();

    const daytona = new Daytona();

    let name: string;
    let gitUrl: string;
    let configPath: string;
    let snapshotName: string | undefined;

    if (params) {
        // 非交互模式
        name = params.name ?? DEFAULT_NAME;
        gitUrl = params.gitUrl ?? DEFAULT_GIT_URL;
        configPath = params.config ?? DEFAULT_CONFIG_PATH;
        snapshotName = params.snapshot || undefined;
        if (snapshotName) {
            console.log(`[create] 快照: ${snapshotName}`);
        }
    } else {
        console.log("\n创建 Daytona Sandbox\n");

        // 选择快照（仅 daytona- 前缀）
        const snapshots = await daytona.snapshot.list(1, 50);
        const filtered = snapshots.items.filter((s) => s.name.startsWith("daytona-"));
        if (filtered.length > 0) {
            const choice = await search<string | undefined>({
                message: "选择快照（回车跳过则使用默认）",
                source: async (input) => {
                    const opts: { name: string; value: string | undefined; description: string }[] = [
                        { name: "（跳过，使用默认快照）", value: undefined, description: "" },
                    ];
                    const term = (input ?? "").toLowerCase();
                    for (const s of filtered) {
                        if (term && !s.name.toLowerCase().includes(term)) continue;
                        opts.push({
                            name: s.name,
                            value: s.name,
                            description: `${s.imageName ?? ""}  ${s.state}`,
                        });
                    }
                    return opts.slice(0, 20);
                },
            });
            snapshotName = choice ?? undefined;
        }

        // 填表
        name = await input({ message: "沙箱名称", default: DEFAULT_NAME });
        gitUrl = await input({ message: "Git 仓库地址", default: DEFAULT_GIT_URL });
        configPath = await input({
            message: "peri 配置文件（本地路径，将传输到沙箱内）",
            default: DEFAULT_CONFIG_PATH,
        });

        console.log("\n即将执行:\n");
        console.log(`  沙箱名称:                  ${name}`);
        console.log(`  Git 仓库:                  ${gitUrl}`);
        console.log(`  peri 配置（本地传沙箱）:    ${configPath}`);
        if (snapshotName) console.log(`  快照:                      ${snapshotName}`);
        const ok = await confirm({ message: "\n确认创建?", default: true });
        if (!ok) { console.log("已取消。"); return; }
    }

    // 检查是否已存在
    const existing = await findSandbox(name);
    if (existing) {
        console.log(`\nSandbox "${name}" 已存在 (${existing.id})，跳过创建`);
        return;
    }

    const config = loadConfig(configPath);

    // Step 1: 创建 Sandbox
    const s1 = ora("创建沙箱...").start();
    let sandbox;
    try {
        sandbox = await daytona.create({
            name,
            language: "typescript",
            autoStopInterval: 0,
            ...(snapshotName ? { snapshot: snapshotName } : {}),
        });
        s1.succeed(`沙箱已创建: ${sandbox.id}`);
    } catch (err) {
        s1.fail(`创建失败: ${err instanceof Error ? err.message : err}`);
        process.exit(1);
    }

    // Step 2: Clone 仓库
    const s2 = ora("克隆仓库...").start();
    try {
        await sandbox.git.clone(gitUrl, MOUNT_DIR, "main");
        s2.succeed(`已克隆: ${gitUrl} → ${MOUNT_DIR}`);
    } catch (err) {
        s2.fail(`克隆失败: ${err instanceof Error ? err.message : err}`);
        process.exit(1);
    }

    // Step 3: 安装 peri CLI + 写入配置
    const s3 = ora("安装 peri CLI + 写入配置").start();
    try {
        await executeCommandList(sandbox, [
            "curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash",
            `mkdir -p ${MOUNT_DIR}/.peri && cat <<'EOF' > ${MOUNT_DIR}/.peri/settings.json\n${JSON.stringify(config, null, 2)}\nEOF`,
        ], { spinner: { text: "安装 peri CLI", suffixText: "curl + mkdir" } });
        s3.succeed("peri CLI 已安装，配置已写入");
    } catch (err) {
        s3.fail(`安装失败: ${err instanceof Error ? err.message : err}`);
        process.exit(1);
    }

    console.log(`\n创建完成!`);
    console.log(`  Sandbox: ${sandbox.id}`);
    console.log(`  peri 路径: ${MOUNT_DIR}`);
    console.log(`\n现在可以运行: peri-sandbox ask "你的问题"`);
}
