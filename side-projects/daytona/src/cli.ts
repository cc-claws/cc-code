#!/usr/bin/env bun
// ---------------------------------------------------------------------------
// cli.ts —— peri-sandbox CLI 入口
// ---------------------------------------------------------------------------
import { Command } from "commander";
import { runInit } from "./commands/init";
import { runCreate } from "./commands/create";
import { runList } from "./commands/list";
import { askPeri } from "./commands/ask";
import { parseParamsArg } from "./daytona-helpers";

const program = new Command();

program
    .name("peri-sandbox")
    .description("在 Daytona 沙箱中运行 peri AI Agent")
    .version("0.1.0");

program
    .command("init")
    .description("初始化 Daytona 连接")
    .option("--params <json>", "JSON 参数，跳过交互（{apiKey, apiUrl}）")
    .action(async (opts) => {
        await runInit(opts.params ? parseParamsArg(opts.params) : undefined);
    });

program
    .command("create")
    .description("创建新沙箱")
    .option("--params <json>", "JSON 参数，跳过交互（{name, gitUrl, snapshot, config}）")
    .action(async (opts) => {
        await runCreate(opts.params ? parseParamsArg(opts.params) : undefined);
    });

program
    .command("list")
    .alias("ls")
    .description("列出所有沙箱")
    .action(async () => {
        await runList();
    });

program
    .command("ask")
    .description("向 peri AI Agent 发送单轮问答")
    .argument("<prompt>", "要发给 peri 的问题")
    .option("--sandbox <name>", "指定沙箱名称或 ID")
    .action(async (prompt, opts) => {
        await askPeri({
            sandbox: opts.sandbox,
            prompt,
        });
    });

program.parseAsync().catch((err) => {
    console.error("错误:", err instanceof Error ? err.message : err);
    process.exit(1);
});
