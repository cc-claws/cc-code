//! Read 工具缺失 file_path 的定向分析。
//! 用法: bun run src/read-missing-path.ts

import chalk from "chalk";
import { DataLoader } from "./utils/data_loader.js";
import { printHeader, printSection, printMetric, printTable, printFinding } from "./utils/report.js";
import type { DefectReport } from "./types.js";

interface MissingPathCase {
  threadId: string;
  threadTitle: string;
  msgIdx: number;
  totalMsgs: number;
  arguments: Record<string, unknown>;
  prevUserText: string;    // 前一条 user 消息
  prevAssistantOps: string; // 前一条 assistant 的工具调用
  nextToolResult: string;   // 后续 tool result（如果有）
  hasFilePath: boolean;     // 参数里有其他路径相关字段吗？
}

function main() {
  const loader = new DataLoader();
  try {
    printHeader("Read 工具缺失 file_path 定向分析");
    
    const threads = loader.loadAllThreads().filter(t => t.hidden === 0);
    const allMissing: MissingPathCase[] = [];
    const allReadCalls: { ok: number; missing: number; zeroArg: number } = { ok: 0, missing: 0, zeroArg: 0 };

    for (const thread of threads) {
      const messages = loader.loadMessages(thread.id);

      for (let i = 0; i < messages.length; i++) {
        const msg = messages[i];
        if (msg.role !== "assistant") continue;

        const parsed = DataLoader.parseContent(msg.content);
        const calls = DataLoader.extractToolCalls(parsed);
        const readCalls = calls.filter(tc => tc.name === "Read");

        for (const tc of readCalls) {
          const args = tc.arguments || {};
          const hasPath = "file_path" in args && typeof args.file_path === "string" && args.file_path.length > 0;
          const hasAltPath = "path" in args && typeof args.path === "string" && (args.path as string).length > 0;
          
          if (hasPath || hasAltPath) {
            allReadCalls.ok++;
            continue;
          }

          // 缺失 file_path
          const argCount = Object.keys(args).length;
          if (argCount === 0) allReadCalls.zeroArg++;
          allReadCalls.missing++;

          // 找上下文
          let prevUserText = "";
          for (let j = i - 1; j >= 0; j--) {
            if (messages[j].role === "user") {
              const p = DataLoader.parseContent(messages[j].content);
              if (p && "content" in p) {
                const c = (p as any).content;
                prevUserText = (typeof c === "string" ? c : 
                  (Array.isArray(c) ? c.filter((b:any)=>b.type==="text").map((b:any)=>b.text||"").join("") : "")
                ).slice(0, 200);
              }
              break;
            }
          }

          let prevAssistantOps = "";
          for (let j = i - 1; j >= 0; j--) {
            if (messages[j].role === "assistant") {
              const p = DataLoader.parseContent(messages[j].content);
              const prevCalls = DataLoader.extractToolCalls(p);
              prevAssistantOps = prevCalls.map(c => `${c.name}(${JSON.stringify(c.arguments).slice(0, 80)})`).join(", ");
              break;
            }
          }

          // 后续 tool result
          let nextToolResult = "无对应 result";
          for (let j = i + 1; j < messages.length && j < i + 5; j++) {
            if (messages[j].role === "tool") {
              const p = DataLoader.parseContent(messages[j].content);
              const err = DataLoader.parseToolError(p);
              if (err) {
                nextToolResult = err.isError ? `ERROR: ${err.content.slice(0, 120)}` : `OK: ${err.content.slice(0, 120)}`;
              }
              break;
            }
          }

          allMissing.push({
            threadId: thread.id,
            threadTitle: (thread.title || "").slice(0, 80),
            msgIdx: i + 1,
            totalMsgs: messages.length,
            arguments: args,
            prevUserText,
            prevAssistantOps,
            nextToolResult,
            hasFilePath: false,
          });
        }
      }
    }

    // ── 报告 ──
    printMetric("Read 调用总数", allReadCalls.ok + allReadCalls.missing);
    printMetric("  正常调用（有 file_path）", allReadCalls.ok);
    printMetric("  缺失 file_path", `${allReadCalls.missing} (${(allReadCalls.missing / (allReadCalls.ok + allReadCalls.missing) * 100).toFixed(2)}%)`);
    if (allReadCalls.zeroArg > 0) {
      printMetric("    其中零参调用（完全空）", allReadCalls.zeroArg);
    }

    if (allMissing.length === 0) {
      console.log(chalk.green("\n  未发现缺失 file_path 的 Read 调用。"));
      return;
    }

    // 参数分析
    printSection("缺失调用携带的非标准参数");
    const argKeys = new Map<string, number>();
    for (const m of allMissing) {
      for (const key of Object.keys(m.arguments)) {
        argKeys.set(key, (argKeys.get(key) || 0) + 1);
      }
    }
    if (argKeys.size > 0) {
      for (const [key, count] of [...argKeys.entries()].sort((a,b)=>b[1]-a[1])) {
        printMetric(`  "${key}"`, count);
      }
    } else {
      console.log(chalk.gray("  所有缺失调用均为零参数（完全空 {}）"));
    }

    // Top cases
    printSection(`缺失详情（共 ${allMissing.length} 例，展示前 20）`);
    const rows = allMissing.slice(0, 20).map(m => [
      m.threadId.slice(0, 10),
      m.threadTitle.slice(0, 30),
      `#${m.msgIdx}/${m.totalMsgs}`,
      JSON.stringify(m.arguments).slice(0, 60),
      m.prevUserText.slice(0, 40),
      m.prevAssistantOps.slice(0, 40),
      m.nextToolResult.slice(0, 40),
    ]);
    printTable(["Session", "标题", "位置", "参数", "前user消息", "前assistant操作", "Tool Result"], rows);

    // 按前一条 assistant 操作分类
    printSection("缺失发生时的前一步操作");
    const prevOps = new Map<string, number>();
    for (const m of allMissing) {
      if (m.prevAssistantOps.length === 0) {
        prevOps.set("(无前一步 — 首条assistant)", (prevOps.get("(无前一步 — 首条assistant)") || 0) + 1);
      } else {
        // 提取工具名
        const toolMatch = m.prevAssistantOps.match(/^(\w+)/);
        const tool = toolMatch ? toolMatch[1] : "其他";
        prevOps.set(tool, (prevOps.get(tool) || 0) + 1);
      }
    }
    for (const [tool, count] of [...prevOps.entries()].sort((a,b)=>b[1]-a[1])) {
      printMetric(`  ${tool}`, count);
    }

    // 按 tool result 结果分类
    printSection("缺失调用的执行结果");
    const results = new Map<string, number>();
    for (const m of allMissing) {
      if (m.nextToolResult.startsWith("ERROR:")) {
        const errType = m.nextToolResult.slice(6, 60).trim();
        results.set(`ERROR: ${errType}`, (results.get(`ERROR: ${errType}`) || 0) + 1);
      } else if (m.nextToolResult.startsWith("OK:")) {
        results.set("OK (成功)", (results.get("OK (成功)") || 0) + 1);
      } else {
        results.set("无对应 result", (results.get("无对应 result") || 0) + 1);
      }
    }
    for (const [r, count] of [...results.entries()].sort((a,b)=>b[1]-a[1])) {
      printMetric(`  ${r}`, count);
    }

  } finally {
    loader.close();
  }
}

main();
