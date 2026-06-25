//! 全工具缺参深度分析 v2。
//! 用法: bun run src/missing-params.ts

import chalk from "chalk";
import { DataLoader } from "./utils/data_loader.js";
import { printHeader, printSection, printMetric, printTable } from "./utils/report.js";

const READ_PROPS = ["file_path", "offset", "limit", "pages"];
const EDIT_PROPS = ["file_path", "old_string", "new_string", "replace_all"];
const WRITE_PROPS = ["file_path", "content"];
const GREP_PROPS = ["pattern", "path", "glob", "type", "output_mode", "-i", "-n", "head_limit", "offset", "fixed_strings"];

const TOOL_SPECS: Record<string, { required: string[]; props: string[] }> = {
  Read:       { required: ["file_path"],              props: READ_PROPS },
  Edit:       { required: ["file_path","old_string","new_string"], props: EDIT_PROPS },
  Write:      { required: ["file_path","content"],     props: WRITE_PROPS },
  Grep:       { required: ["pattern"],                 props: GREP_PROPS },
  Glob:       { required: ["pattern"],                 props: ["pattern", "path"] },
  Bash:       { required: ["command"],                 props: ["command", "description", "timeout", "run_in_background"] },
  WebFetch:   { required: ["url"],                     props: ["url", "prompt"] },
  WebSearch:  { required: ["query"],                   props: ["query", "num_results"] },
};

function main() {
  const loader = new DataLoader();
  try {
    printHeader("全工具缺参深度分析");

    const threads = loader.loadAllThreads().filter(t => t.hidden === 0);

    const stats: Record<string, {
      total: number; missingAny: number; emptyArgs: number;
      paramFreq: Map<string, number>;
    }> = {};

    const missingCases: {
      tool: string; threadId: string; title: string; missing: string[];
      args: Record<string, unknown>; prevUser: string;
    }[] = [];

    // 收集所有工具名
    const allToolNames = new Set<string>();

    for (const thread of threads) {
      const messages = loader.loadMessages(thread.id);
      for (let i = 0; i < messages.length; i++) {
        const msg = messages[i];
        if (msg.role !== "assistant") continue;
        const parsed = DataLoader.parseContent(msg.content);
        const calls = DataLoader.extractToolCalls(parsed);

        for (const tc of calls) {
          const name = tc.name;
          allToolNames.add(name);
          const spec = TOOL_SPECS[name];
          if (!spec) continue;

          if (!stats[name]) {
            stats[name] = { total: 0, missingAny: 0, emptyArgs: 0, paramFreq: new Map() };
          }
          const s = stats[name];
          s.total++;

          const args = tc.arguments || {};
          const argKeys = Object.keys(args);
          for (const k of argKeys) {
            s.paramFreq.set(k, (s.paramFreq.get(k) || 0) + 1);
          }
          if (argKeys.length === 0) s.emptyArgs++;

          // 缺失必要参数（参数不存在，或存在但值为空字符串）
          const missing = spec.required.filter(r => {
            if (!(r in args)) return true;
            const v = args[r];
            return typeof v === "string" && (v as string).trim().length === 0;
          });

          if (missing.length > 0) {
            s.missingAny++;

            let prevUser = "";
            for (let j = i - 1; j >= 0; j--) {
              if (messages[j].role === "user") {
                const p = DataLoader.parseContent(messages[j].content);
                if (p && "content" in p) {
                  const c = (p as any).content;
                  prevUser = (typeof c === "string" ? c : "").slice(0, 150);
                }
                break;
              }
            }

            missingCases.push({
              tool: name, threadId: thread.id,
              title: (thread.title || "").slice(0, 60),
              missing, args, prevUser,
            });
          }
        }
      }
    }

    // 先列出所有发现的工具名
    printSection("数据中出现的工具名");
    console.log(chalk.gray(`  ${[...allToolNames].sort().join(", ")}`));

    // 逐个工具报告
    for (const [name, spec] of Object.entries(TOOL_SPECS)) {
      const s = stats[name];
      if (!s || s.total === 0) {
        console.log(chalk.gray(`\n  ${name}: 无调用`));
        continue;
      }
      printSection(`${name} (总调用 ${s.total})`);
      printMetric("缺必要参数", `${s.missingAny} (${(s.missingAny/s.total*100).toFixed(2)}%)`);
      if (s.emptyArgs > 0) printMetric("⚠ 完全空参数 {}", s.emptyArgs);

      console.log(chalk.gray("  各参数出现频率:"));
      for (const prop of spec.props) {
        const cnt = s.paramFreq.get(prop) || 0;
        const pct = (cnt / s.total * 100).toFixed(0);
        const icon = spec.required.includes(prop)
          ? (cnt === s.total ? chalk.green("✓") : chalk.red("✗ "))
          : "  ";
        console.log(`    ${icon} ${prop}: ${cnt}/${s.total} (${pct}%)`);
      }

      // 非标准参数
      const extraParams = [...s.paramFreq.keys()].filter(k => !spec.props.includes(k));
      if (extraParams.length > 0) {
        console.log(chalk.yellow(`  非标准参数: ${extraParams.map(k => `${k}(${s.paramFreq.get(k)})`).join(", ")}`));
      }
    }

    // 缺参案例
    if (missingCases.length > 0) {
      printSection(`\n缺参案例详情（共 ${missingCases.length} 例）`);
      const rows = missingCases.slice(0, 30).map(c => [
        c.tool, c.missing.join(","),
        JSON.stringify(c.args).slice(0, 70),
        c.prevUser.slice(0, 50),
      ]);
      printTable(["工具", "缺失", "实际参数", "前 user 消息"], rows);

      // 分工具深入
      const byTool = new Map<string, typeof missingCases>();
      for (const c of missingCases) {
        if (!byTool.has(c.tool)) byTool.set(c.tool, []);
        byTool.get(c.tool)!.push(c);
      }
      for (const [tool, cases] of byTool) {
        printSection(`${tool} 缺参深入（${cases.length} 例）`);
        for (const c of cases.slice(0, 5)) {
          console.log(chalk.gray(`  [${c.threadId.slice(0,12)}] ${c.title.slice(0,40)}`));
          console.log(chalk.yellow(`  缺失: ${c.missing.join(", ")}`));
          console.log(chalk.gray(`  参数: ${JSON.stringify(c.args).slice(0, 150)}`));
          console.log(chalk.gray(`  前 user: ${c.prevUser.slice(0, 100)}`));
          console.log();
        }
      }
    } else {
      console.log(chalk.green("\n  未发现任何工具缺失必要参数。"));
    }

  } finally {
    loader.close();
  }
}

main();
