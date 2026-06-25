//! 用户行为深度洞察实验 v2 — 方法论修正版。
//!
//! 废弃指标:
//!   旧 E2（cwd≠用户，无法修复）
//!   旧 D1（9 个样本无统计意义，分段武断）
//!
//! 保留 + 修正:
//!   C1  — 编辑返工率 + 自纠错率（区分"用户不满意重做"和"agent自发纠错"）
//!   A1  — compacted-aware 消息衰减（Mann-Kendall 趋势检验）
//!   X-001 — 非 compacted 会话中工具错误的影响
//!
//! 新增:
//!   会话生命周期位置分析（越用越熟练还是越用越疲惫？）
//!
//! 用法: bun run src/user-insight-experiment.ts

import chalk from "chalk";
import { DataLoader } from "./utils/data_loader.js";
import { printHeader, printSection, printMetric, printTable, printFinding, printProgressBar, printWarning } from "./utils/report.js";
import type { DefectReport, ThreadRow, MessageRow } from "./types.js";

const SEP = chalk.gray("─".repeat(80));

// ════════════════════════════════════════════════════════════════════
//  统计工具函数
// ════════════════════════════════════════════════════════════════════

/** Mann-Kendall 趋势检验。返回 z-score 和双边 p 值。 */
function mannKendall(values: number[]): { z: number; p: number; slope: number; trend: "increasing" | "decreasing" | "none" } {
  const n = values.length;
  if (n < 3) return { z: 0, p: 1, slope: 0, trend: "none" };

  // S 统计量
  let S = 0;
  for (let i = 0; i < n - 1; i++) {
    for (let j = i + 1; j < n; j++) {
      if (values[j] > values[i]) S++;
      else if (values[j] < values[i]) S--;
    }
  }

  // 方差（考虑 ties）
  const uniqueVals = [...new Set(values)];
  let varS = (n * (n - 1) * (2 * n + 5)) / 18;
  for (const val of uniqueVals) {
    const count = values.filter((v) => v === val).length;
    if (count > 1) {
      varS -= (count * (count - 1) * (2 * count + 5)) / 18;
    }
  }

  // z-score
  let z = 0;
  if (S > 0) z = (S - 1) / Math.sqrt(varS);
  else if (S < 0) z = (S + 1) / Math.sqrt(varS);

  // p 值（双边，正态近似）
  const p = 2 * (1 - normalCDF(Math.abs(z)));

  // Theil-Sen 斜率
  const slopes: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    for (let j = i + 1; j < n; j++) {
      slopes.push((values[j] - values[i]) / (j - i));
    }
  }
  slopes.sort((a, b) => a - b);
  const slope = slopes.length % 2 === 0
    ? (slopes[slopes.length / 2 - 1] + slopes[slopes.length / 2]) / 2
    : slopes[Math.floor(slopes.length / 2)];

  let trend: "increasing" | "decreasing" | "none" = "none";
  if (p < 0.05 && slope < -0.5) trend = "decreasing";
  else if (p < 0.05 && slope > 0.5) trend = "increasing";

  return { z, p, slope, trend };
}

/** 标准正态 CDF 近似（Abramowitz & Stegun） */
function normalCDF(x: number): number {
  const a1 = 0.254829592, a2 = -0.284496736, a3 = 1.421413741;
  const a4 = -1.453152027, a5 = 1.061405429, pVal = 0.3275911;
  const sign = x >= 0 ? 1 : -1;
  x = Math.abs(x) / Math.sqrt(2);
  const t = 1 / (1 + pVal * x);
  const y = 1 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * Math.exp(-x * x);
  return 0.5 * (1 + sign * y);
}

// ════════════════════════════════════════════════════════════════════
//  Compacted 会话识别
// ════════════════════════════════════════════════════════════════════

const COMPACT_MARKERS = [
  "以下是之前对话的摘要",
  "Here's a summary of the previous conversation",
  "对话摘要：",
  "## 对话摘要",
  "Conversation Summary",
  "## Summary of",
];

/** 判断一个会话是否经过 compaction。
 *  三重检测：(1) 线程标题含 "Compact:" 前缀 (2) 首条 user 消息含摘要标记 (3) 任一条 system 消息含 compact 特征 */
function isCompactedSession(loader: DataLoader, thread: ThreadRow): boolean {
  // 特征 1：线程标题包含 "Compact:" 前缀（最可靠）
  const title = (thread.title || "").toLowerCase();
  if (title.startsWith("compact:") || title.includes("compact:")) return true;

  const messages = loader.loadMessages(thread.id);

  // 特征 2：首条或最长的 user 消息含摘要标记
  let longestUserText = "";
  for (const msg of messages) {
    if (msg.role !== "user") continue;
    const text = extractUserText(msg);
    if (text.length > longestUserText.length) longestUserText = text;
  }

  if (longestUserText.length >= 500) {
    const lower = longestUserText.toLowerCase();
    if (COMPACT_MARKERS.some((marker) => lower.includes(marker.toLowerCase()))) {
      return true;
    }
  }

  // 特征 3：任一条 system 消息含 compact 特征
  for (const msg of messages) {
    if (msg.role !== "system") continue;
    const parsed = DataLoader.parseContent(msg.content);
    let text = "";
    if (parsed && "content" in parsed) {
      const content = (parsed as any).content;
      if (typeof content === "string") text = content;
      else if (Array.isArray(content)) {
        text = content.filter((b: any) => b.type === "text").map((b: any) => b.text || "").join("");
      }
    }
    if (text.length > 500 && COMPACT_MARKERS.some((m) => text.toLowerCase().includes(m.toLowerCase()))) {
      return true;
    }
  }

  return false;
}

// ════════════════════════════════════════════════════════════════════
//  用户消息文本提取
// ════════════════════════════════════════════════════════════════════

function extractUserText(msg: MessageRow): string {
  if (msg.role !== "user") return "";
  const parsed = DataLoader.parseContent(msg.content);
  if (!parsed || !("content" in parsed)) return "";
  const content = (parsed as any).content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .filter((b: any) => b.type === "text")
      .map((b: any) => b.text || "")
      .join("");
  }
  return "";
}

// ════════════════════════════════════════════════════════════════════
// ━━━ 主入口 ━━━
// ════════════════════════════════════════════════════════════════════

export function runUserInsightExperiment(loader: DataLoader): DefectReport[] {
  const reports: DefectReport[] = [];

  printHeader("用户行为深度洞察实验 v2（方法论修正版）");
  console.log(chalk.gray("  废弃: E2（cwd≠用户）, D1（9样本无统计意义）"));
  console.log(chalk.gray("  修正: C1（返工率+自纠错率）, A1（Mann-Kendall+compacted排除）"));
  console.log(chalk.gray("  新增: 会话生命周期位置分析"));
  console.log(chalk.gray("  约束: 仅分析非 compacted 会话，统计检验替代魔数阈值\n"));

  // ━━━ 数据加载 ━━━
  console.time("  数据预加载");
  const allThreads = loader.loadAllThreads();
  const visibleThreads = allThreads.filter((t) => t.hidden === 0);

  // 识别并分类会话
  const compactedIds = new Set<string>();
  const nonCompactedThreads: ThreadRow[] = [];
  for (const t of visibleThreads) {
    if (isCompactedSession(loader, t)) compactedIds.add(t.id);
    else nonCompactedThreads.push(t);
  }

  console.log(chalk.gray(`  可见会话 ${visibleThreads.length} | compacted ${compactedIds.size} (${(compactedIds.size / visibleThreads.length * 100).toFixed(1)}%) | 非compacted ${nonCompactedThreads.length}`));
  console.timeEnd("  数据预加载");

  // ━━━ C1: 编辑返工率 + 自纠错率 ━━━
  console.time("  C1 编辑分析");
  reports.push(...analyzeEditRework(loader, nonCompactedThreads));
  console.timeEnd("  C1 编辑分析");

  // ━━━ A1: compacted-aware 消息衰减 ━━━
  console.time("  A1 消息衰减");
  reports.push(...analyzeMessageTrend(loader, nonCompactedThreads));
  console.timeEnd("  A1 消息衰减");

  // ━━━ X-001: 非 compacted 错误影响 ━━━
  console.time("  X-001 错误影响");
  reports.push(...analyzeErrorImpact(loader, nonCompactedThreads, compactedIds));
  console.timeEnd("  X-001 错误影响");

  // ━━━ 新增: 会话生命周期位置分析 ━━━
  console.time("  生命周期分析");
  reports.push(...analyzeSessionLifecycle(loader, visibleThreads, compactedIds));
  console.timeEnd("  生命周期分析");

  return reports;
}

// ════════════════════════════════════════════════════════════════════
//  C1: 编辑返工率 vs 自纠错率
// ════════════════════════════════════════════════════════════════════

const NEGATION_PATTERNS = [
  /不对/, /改成/, /应该是/, /换成/, /换回/, /撤销/, /撤回/,
  /不要这样/, /重新/, /再来/, /再改/, /不对的/,
  /no,? /i, /wrong/i, /instead/i, /revert/i, /undo/i,
  /should be/i, /change to/i, /replace with/i,
];

interface EditRecord {
  filePath: string;
  threadId: string;
  totalEdits: number;               // Agent 在此文件上的总编辑次数
  reworkEdits: number;              // user 发修正指令后的编辑（强返工信号）
  selfCorrectEdits: number;         // agent 自发纠错（两次编辑之间无 user 消息）
  reworkDetails: { from: number; to: number; reason: string }[];
}

function analyzeEditRework(loader: DataLoader, threads: ThreadRow[]): DefectReport[] {
  printSection("C1 | 编辑返工率 vs 自纠错率");
  console.log(chalk.gray("  方法论: 仅分析非 compacted 会话。"));
  console.log(chalk.gray("  返工 = user 发修正/否定指令后 agent 重新编辑同一文件（强信号）"));
  console.log(chalk.gray("  自纠错 = agent 自发重新编辑，无 user 消息介入（值得监控，非用户不满意）\n"));

  const editMap = new Map<string, EditRecord>(); // key: threadId::filePath

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);

    // 收集所有 assistant 的 Edit/Write 调用及其位置
    const editOps: { idx: number; filePath: string; toolName: string }[] = [];
    const userMsgs: { idx: number; text: string }[] = [];

    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      if (msg.role === "user") {
        const text = extractUserText(msg);
        if (text.length > 0) userMsgs.push({ idx: i, text });
      }
      if (msg.role === "assistant") {
        const parsed = DataLoader.parseContent(msg.content);
        const calls = DataLoader.extractToolCalls(parsed);
        for (const tc of calls) {
          if (tc.name === "Write" || tc.name === "Edit") {
            const fp = (tc.arguments?.file_path as string) || (tc.arguments?.path as string) || "unknown";
            editOps.push({ idx: i, filePath: fp, toolName: tc.name });
          }
        }
      }
    }

    // 分析每个文件的编辑序列
    const fileEdits = new Map<string, number[]>();
    for (const op of editOps) {
      if (!fileEdits.has(op.filePath)) fileEdits.set(op.filePath, []);
      fileEdits.get(op.filePath)!.push(op.idx);
    }

    for (const [filePath, indices] of fileEdits) {
      const key = `${thread.id}::${filePath}`;
      if (!editMap.has(key)) {
        editMap.set(key, {
          filePath, threadId: thread.id,
          totalEdits: 0, reworkEdits: 0, selfCorrectEdits: 0,
          reworkDetails: [],
        });
      }
      const rec = editMap.get(key)!;
      rec.totalEdits += indices.length;

      // 对每次后续编辑判断是"返工"还是"自纠错"
      for (let ei = 1; ei < indices.length; ei++) {
        const prevIdx = indices[ei - 1];
        const currIdx = indices[ei];

        // 查找两次编辑之间是否有 user 消息
        const interveningUsers = userMsgs.filter(
          (u) => u.idx > prevIdx && u.idx < currIdx
        );

        if (interveningUsers.length > 0) {
          // 有 user 消息 → 检查是否含修正/否定意图
          const hasNegation = interveningUsers.some((u) =>
            NEGATION_PATTERNS.some((p) => p.test(u.text))
          );
          if (hasNegation) {
            rec.reworkEdits++;
            rec.reworkDetails.push({
              from: prevIdx + 1, to: currIdx + 1,
              reason: interveningUsers[0].text.slice(0, 80),
            });
          }
        } else {
          // 无 user 消息 → agent 自纠错
          rec.selfCorrectEdits++;
        }
      }
    }
  }

  const allEdits = [...editMap.values()];
  const totalEdits = allEdits.reduce((s, e) => s + e.totalEdits, 0);
  const totalRework = allEdits.reduce((s, e) => s + e.reworkEdits, 0);
  const totalSelfCorrect = allEdits.reduce((s, e) => s + e.selfCorrectEdits, 0);

  // 首次编辑（第一次写文件）的数量
  const firstEdits = allEdits.filter((e) => e.totalEdits > 0).length;

  printMetric("文件编辑总数", totalEdits);
  printMetric("涉及的文件数", allEdits.length);
  printMetric("返工次数（用户驱动）", `${totalRework} (${(totalRework / (totalEdits || 1) * 100).toFixed(1)}%)`);
  printMetric("自纠错次数（Agent自发）", `${totalSelfCorrect} (${(totalSelfCorrect / (totalEdits || 1) * 100).toFixed(1)}%)`);

  const reworkRate = totalRework / (totalEdits || 1);
  const selfCorrectRate = totalSelfCorrect / (totalEdits || 1);
  printProgressBar("返工率（用户不满意重做）", reworkRate);
  printProgressBar("自纠错率（Agent自发修正）", selfCorrectRate);

  // 返工热点文件
  const reworkFiles = allEdits
    .filter((e) => e.reworkEdits > 0)
    .sort((a, b) => b.reworkEdits - a.reworkEdits);
  if (reworkFiles.length > 0) {
    printSection("返工热点文件（Top 10）");
    const rows = reworkFiles.slice(0, 10).map((e) => [
      e.filePath.split("/").pop() || e.filePath,
      String(e.totalEdits),
      String(e.reworkEdits),
      `${(e.reworkEdits / (e.totalEdits || 1) * 100).toFixed(0)}%`,
      e.reworkDetails[0]?.reason.slice(0, 40) || "",
    ]);
    printTable(["文件", "总编辑", "返工", "返工占比", "触发例"], rows);
  }

  const reports: DefectReport[] = [];

  if (reworkRate > 0.05 && totalEdits > 50) {
    reports.push({
      id: "C1-001",
      severity: reworkRate > 0.1 ? "high" : "medium",
      category: "编辑返工",
      title: `编辑返工率 ${(reworkRate * 100).toFixed(1)}%：${totalRework} 次用户驱动的重做`,
      description: `${totalEdits} 次文件编辑中，${totalRework} 次是用户发修正指令后触发的。同时 agent 自发纠错 ${totalSelfCorrect} 次。`,
      evidence: reworkFiles.slice(0, 3).map((e) =>
        `${e.filePath.split("/").pop()}: ${e.reworkEdits}/${e.totalEdits}次返工`
      ),
      affectedSessions: [...new Set(reworkFiles.map((e) => e.threadId))],
      recommendation: "分析返工热点文件的编辑内容差异。在第一次编辑后主动列出假设和边界条件，降低用户纠正需求。",
      confidence: 0.75,
    });
  }

  return reports;
}

// ════════════════════════════════════════════════════════════════════
//  A1: Compacted-aware 消息衰减 (Mann-Kendall)
// ════════════════════════════════════════════════════════════════════

interface TrendResult {
  threadId: string;
  title: string;
  rounds: number;
  lengths: number[];
  mkZ: number;
  mkP: number;
  slope: number;        // Theil-Sen, 每轮字数变化
  trend: string;        // deep-decrease / decrease / stable / increase
}

function analyzeMessageTrend(loader: DataLoader, threads: ThreadRow[]): DefectReport[] {
  printSection("A1 | 消息长度趋势（Mann-Kendall 检验）");
  console.log(chalk.gray("  方法论: 排除 compacted 会话。Mann-Kendall 非参数趋势检验 + Theil-Sen 斜率。"));
  console.log(chalk.gray("  显著递减: p < 0.05 且斜率 < -0.5 字/轮。\n"));

  const results: TrendResult[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const userLengths: number[] = [];

    for (const msg of messages) {
      if (msg.role !== "user") continue;
      const text = extractUserText(msg);
      if (text.length > 0) userLengths.push(text.length);
    }

    if (userLengths.length < 4) continue; // MK 检验至少需要 4+ 点才有意义

    const mk = mannKendall(userLengths);

    let trendLabel: string;
    if (mk.p < 0.01 && mk.slope < -0.5) trendLabel = "deep-decrease";
    else if (mk.p < 0.05 && mk.slope < -0.5) trendLabel = "decrease";
    else if (mk.p < 0.05 && mk.slope > 0.5) trendLabel = "increase";
    else trendLabel = "stable";

    results.push({
      threadId: thread.id,
      title: (thread.title || "").slice(0, 60),
      rounds: userLengths.length,
      lengths: userLengths,
      mkZ: mk.z,
      mkP: mk.p,
      slope: mk.slope,
      trend: trendLabel,
    });
  }

  // 分布统计
  const sigDecrease = results.filter((r) => r.trend === "decrease" || r.trend === "deep-decrease");
  const deepDecrease = results.filter((r) => r.trend === "deep-decrease");
  const sigIncrease = results.filter((r) => r.trend === "increase");

  printMetric("可分析会话（≥4轮）", results.length);
  printMetric("显著递减 (p<0.05)", `${sigDecrease.length} (${(sigDecrease.length / (results.length || 1) * 100).toFixed(1)}%)`);
  printMetric("  其中极显著 (p<0.01)", `${deepDecrease.length} (${(deepDecrease.length / (results.length || 1) * 100).toFixed(1)}%)`);
  printMetric("显著递增 (p<0.05)", `${sigIncrease.length} (${(sigIncrease.length / (results.length || 1) * 100).toFixed(1)}%)`);
  printMetric("无显著趋势", `${results.length - sigDecrease.length - sigIncrease.length}`);

  // 效应量：递减会话的中位衰减速度
  if (sigDecrease.length > 0) {
    const medianSlope = [...sigDecrease].sort((a, b) => a.slope - b.slope)[Math.floor(sigDecrease.length / 2)].slope;
    printMetric("递减会话中位衰减速度", `${medianSlope.toFixed(1)}`, " 字/轮");
  }

  // Top 递减会话
  if (sigDecrease.length > 0) {
    const top = [...sigDecrease]
      .sort((a, b) => a.slope - b.slope)
      .slice(0, 10);
    printSection("最显著递减会话（Top 10）");
    const rows = top.map((r) => [
      r.threadId.slice(0, 12),
      r.title.slice(0, 35),
      String(r.rounds),
      r.slope.toFixed(1),
      r.mkP.toFixed(4),
      r.lengths.join("→"),
    ]);
    printTable(["Session", "标题", "轮数", "衰减(字/轮)", "p值", "长度序列"], rows);
  }

  const reports: DefectReport[] = [];

  const decPct = sigDecrease.length / (results.length || 1);
  if (decPct > 0.15) {
    reports.push({
      id: "A1-001",
      severity: decPct > 0.25 ? "high" : "medium",
      category: "用户受挫",
      title: `${(decPct * 100).toFixed(0)}% 的非 compacted 会话呈显著消息衰减趋势`,
      description: `${sigDecrease.length}/${results.length} 个会话的消息长度经 Mann-Kendall 检验呈显著递减（p<0.05），其中 ${deepDecrease.length} 个极显著（p<0.01）。排除 compacted 误判后，这是用户耐心/信任下降的真实信号。`,
      evidence: sigDecrease.slice(0, 3).map((r) =>
        `${r.title.slice(0, 30)}: ${r.slope.toFixed(1)}字/轮 (p=${r.mkP.toFixed(3)})`
      ),
      affectedSessions: sigDecrease.map((r) => r.threadId),
      recommendation: "在衰减达到显著水平（p<0.05）的会话中，第3-4轮后主动简化回答策略。增加'我对需求的理解对吗？'自省步骤。",
      confidence: 0.72,
    });
  }

  return reports;
}

// ════════════════════════════════════════════════════════════════════
//  X-001: 非 compacted 会话中工具错误的影响
// ════════════════════════════════════════════════════════════════════

interface ErrorAftermath {
  threadId: string;
  errorIdx: number;
  errorTool: string;
  outcome: "continue" | "stop" | "self_recover" | "short_msg";
  detail: string;
}

function analyzeErrorImpact(
  loader: DataLoader,
  nonCompacted: ThreadRow[],
  compactedIds: Set<string>
): DefectReport[] {
  printSection("X-001 | 工具错误后行为分析");
  console.log(chalk.gray("  方法论: 仅分析非 compacted 会话中可完整追踪的错误链。"));
  console.log(chalk.gray("  自恢复 = 错误后 agent 有后续非 error 消息。停止 = 错误后无 user 跟进且无 agent 自恢复。"));
  console.log(chalk.gray(`  排除 ${compactedIds.size} 个 compacted 会话（compaction 截断后不可追踪）\n`));

  const aftermaths: ErrorAftermath[] = [];

  for (const thread of nonCompacted) {
    const messages = loader.loadMessages(thread.id);

    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      if (msg.role !== "tool") continue;
      if (!msg.content.includes('is_error":true')) continue;

      // 提取工具名
      const parsed = DataLoader.parseContent(msg.content);
      const errInfo = DataLoader.parseToolError(parsed);
      const toolName = errInfo?.toolCallId?.split("::")[0] || "unknown";

      const remaining = messages.slice(i + 1);
      const nextUser = remaining.find((m) => m.role === "user");
      const nextAssistant = remaining.find((m) => m.role === "assistant");

      if (!nextUser && !nextAssistant) {
        // 1. 错误后无任何后续消息 → 真正停止
        aftermaths.push({
          threadId: thread.id, errorIdx: i, errorTool: toolName,
          outcome: "stop", detail: "无后续消息",
        });
      } else if (!nextUser && nextAssistant) {
        // 2. 错误后有 assistant 消息但无 user → agent 自恢复
        aftermaths.push({
          threadId: thread.id, errorIdx: i, errorTool: toolName,
          outcome: "self_recover",
          detail: `assistant 在第${messages.indexOf(nextAssistant) + 1}条恢复`,
        });
      } else if (nextUser) {
        // 3. 错误后有 user 消息
        const text = extractUserText(nextUser);
        if (text.length > 0 && text.length <= 10) {
          aftermaths.push({
            threadId: thread.id, errorIdx: i, errorTool: toolName,
            outcome: "short_msg",
            detail: `用户回复 ≤10字: "${text.slice(0, 20)}"`,
          });
        } else {
          aftermaths.push({
            threadId: thread.id, errorIdx: i, errorTool: toolName,
            outcome: "continue",
            detail: `用户继续对话 (${text.length}字)`,
          });
        }
      }
    }
  }

  const histogram = new Map<string, number>();
  for (const a of aftermaths) histogram.set(a.outcome, (histogram.get(a.outcome) || 0) + 1);

  printMetric("工具错误总数", aftermaths.length);
  for (const [outcome, label] of [
    ["continue", "用户继续对话"],
    ["self_recover", "Agent 自恢复"],
    ["short_msg", "用户发极短消息(≤10字)"],
    ["stop", "会话终止 ⚠️"],
  ] as const) {
    const count = histogram.get(outcome) || 0;
    const pct = (count / (aftermaths.length || 1) * 100).toFixed(1);
    const icon = outcome === "stop" ? chalk.red(" ← 危险信号") : outcome === "short_msg" ? chalk.yellow(" ← 不满信号") : "";
    printMetric(label, `${count} (${pct}%)${icon}`);
  }

  // 按工具分类
  const toolImpact = new Map<string, { total: number; stop: number; short: number }>();
  for (const a of aftermaths) {
    if (!toolImpact.has(a.errorTool)) toolImpact.set(a.errorTool, { total: 0, stop: 0, short: 0 });
    const t = toolImpact.get(a.errorTool)!;
    t.total++;
    if (a.outcome === "stop") t.stop++;
    if (a.outcome === "short_msg") t.short++;
  }

  const rankedTools = [...toolImpact.entries()]
    .filter(([, v]) => v.total >= 3)
    .sort((a, b) => (b[1].stop + b[1].short) / (b[1].total || 1) - (a[1].stop + a[1].short) / (a[1].total || 1));
  if (rankedTools.length > 0) {
    printSection("按工具的致命性排名（停止+短消息占比）");
    const rows = rankedTools.map(([tool, v]) => [
      tool,
      String(v.total),
      `${v.stop}`,
      `${v.short}`,
      `${((v.stop + v.short) / (v.total || 1) * 100).toFixed(0)}%`,
    ]);
    printTable(["工具", "总错误", "终止", "短消息", "致命比率"], rows);
  }

  const reports: DefectReport[] = [];
  const stopCount = histogram.get("stop") || 0;
  const stopRate = stopCount / (aftermaths.length || 1);

  if (stopRate > 0.1) {
    reports.push({
      id: "X-001",
      severity: stopRate > 0.2 ? "high" : "medium",
      category: "错误恢复",
      title: `${(stopRate * 100).toFixed(0)}% 的工具错误导致会话终止（非 compacted 会话基准）`,
      description: `${aftermaths.length} 次可追踪的工具错误中，${stopCount} 次后无 user 跟进且无 agent 自恢复。另有 ${histogram.get("self_recover") || 0} 次 agent 成功自恢复。排除 compacted 干扰，这是工具错误真实致命率的保守下界。`,
      evidence: [
        `自恢复: ${histogram.get("self_recover") || 0}次`,
        `短消息: ${histogram.get("short_msg") || 0}次`,
        `终止: ${stopCount}次`,
      ],
      affectedSessions: [...new Set(aftermaths.filter((a) => a.outcome === "stop").map((a) => a.threadId))],
      recommendation: "最高致命性的工具应优先修复。对 agent 自恢复成功的案例，提取恢复策略用于增强错误恢复提示。",
      confidence: 0.78,
    });
  }

  return reports;
}

// ════════════════════════════════════════════════════════════════════
//  新增: 会话生命周期位置分析
//  问题: 用户是越用越熟练（后段错误率更低）还是越用越疲惫（后段放弃率更高）？
// ════════════════════════════════════════════════════════════════════

interface LifecycleBucket {
  label: string;            // "前20%" / "20-40%" / ...
  sessions: number;
  avgMessages: number;
  avgUserMsgLen: number;
  toolErrorRate: number;
  avgToolsPerTurn: number;
  subAgentRate: number;     // 使用 SubAgent 的会话比例
  shortSessionRate: number; // ≤5条消息的会话比例
}

function analyzeSessionLifecycle(
  loader: DataLoader,
  threads: ThreadRow[],
  compactedIds: Set<string>
): DefectReport[] {
  printSection("新增 | 会话生命周期位置分析");
  console.log(chalk.gray("  研究问题: 随着会话累积，用户和 Agent 的交互模式如何演变？"));
  console.log(chalk.gray("  方法: 按 cwd 分组，取每个 cwd 下会话最多的组，按时序分 5 个等宽桶\n"));

  // 找最活跃的 cwd（大于 20 个会话的才有生命周期分析价值）
  const cwdGroups = new Map<string, ThreadRow[]>();
  for (const t of threads) {
    const cwd = t.cwd || "(unknown)";
    if (!cwdGroups.has(cwd)) cwdGroups.set(cwd, []);
    cwdGroups.get(cwd)!.push(t);
  }

  const viableCwds = [...cwdGroups.entries()]
    .filter(([, ts]) => ts.length >= 20)
    .sort((a, b) => b[1].length - a[1].length);

  if (viableCwds.length === 0) {
    printWarning("无满足 ≥20 会话的项目，跳过生命周期分析", "");
    return [];
  }

  // 分析每个项目的生命周期
  const allBins: { cwd: string; bins: LifecycleBucket[]; totalSessions: number }[] = [];

  for (const [cwd, cwdThreads] of viableCwds.slice(0, 3)) {
    const sorted = cwdThreads.sort(
      (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
    );
    const total = sorted.length;
    const bucketSize = Math.ceil(total / 5);

    const bins: LifecycleBucket[] = [];
    for (let b = 0; b < 5; b++) {
      const start = b * bucketSize;
      const end = Math.min((b + 1) * bucketSize, total);
      const bucket = sorted.slice(start, end);

      let totalMsgs = 0, totalUserLen = 0, totalUserMsgs = 0, totalTools = 0;
      let totalErrors = 0, totalToolMsgs = 0, subAgentSessions = 0, shortSessions = 0;

      for (const t of bucket) {
        const msgs = loader.loadMessages(t.id);
        const subAgents = loader.loadSubAgents(t.id);
        totalMsgs += msgs.length;
        if (subAgents.length > 0) subAgentSessions++;
        if (msgs.length <= 5) shortSessions++;

        for (const m of msgs) {
          if (m.role === "user") {
            const text = extractUserText(m);
            if (text.length > 0) { totalUserLen += text.length; totalUserMsgs++; }
          }
          if (m.role === "tool") {
            totalToolMsgs++;
            if (m.content.includes('is_error":true')) totalErrors++;
          }
          if (m.role === "assistant") {
            const parsed = DataLoader.parseContent(m.content);
            const calls = DataLoader.extractToolCalls(parsed);
            totalTools += calls.length;
          }
        }
      }

      const label = b === 0 ? "前20%" : b === 4 ? "后20%" : `${(b * 20)}-${((b + 1) * 20)}%`;

      bins.push({
        label,
        sessions: bucket.length,
        avgMessages: totalMsgs / (bucket.length || 1),
        avgUserMsgLen: totalUserLen / (totalUserMsgs || 1),
        toolErrorRate: totalErrors / (totalToolMsgs || 1) * 100,
        avgToolsPerTurn: totalTools / (totalUserMsgs || 1),
        subAgentRate: subAgentSessions / (bucket.length || 1) * 100,
        shortSessionRate: shortSessions / (bucket.length || 1) * 100,
      });
    }

    allBins.push({ cwd, bins, totalSessions: total });

    printSection(`${cwd.split("/").pop() || cwd} (${total} 个会话)`);
    const rows = bins.map((b) => [
      b.label,
      String(b.sessions),
      b.avgMessages.toFixed(0),
      b.avgUserMsgLen.toFixed(0),
      b.toolErrorRate.toFixed(1) + "%",
      b.avgToolsPerTurn.toFixed(1),
      b.subAgentRate.toFixed(0) + "%",
      b.shortSessionRate.toFixed(0) + "%",
    ]);
    printTable(
      ["位置", "会话", "均消息", "均用户字", "错误率", "均工具/轮", "SubAgent%", "短会话%"],
      rows
    );
  }

  // 生成报告
  const reports: DefectReport[] = [];

  for (const { cwd, bins } of allBins) {
    const first = bins[0];
    const last = bins[bins.length - 1];
    const shortName = cwd.split("/").pop() || cwd;

    // 错误率上升
    const errorDelta = last.toolErrorRate - first.toolErrorRate;
    if (errorDelta > 5) {
      reports.push({
        id: `LIFE-${shortName}-001`,
        severity: "high",
        category: "生命周期退化",
        title: `${shortName}: 后段会话错误率 ${last.toolErrorRate.toFixed(1)}% vs 前段 ${first.toolErrorRate.toFixed(1)}%（+${errorDelta.toFixed(1)}pp）`,
        description: `随着会话累积，工具错误率上升。可能表明长上下文/累积状态对 Agent 性能有负面影响。`,
        evidence: [
          `前20%: ${first.toolErrorRate.toFixed(1)}%`,
          `后20%: ${last.toolErrorRate.toFixed(1)}%`,
        ],
        affectedSessions: [],
        recommendation: "检查后段会话是否普遍更长（上下文膨胀）。可在会话数达到阈值后自动建议 compact 或开启新会话。",
        confidence: 0.65,
      });
    }

    // 短会话率上升
    const shortDelta = last.shortSessionRate - first.shortSessionRate;
    if (shortDelta > 15) {
      reports.push({
        id: `LIFE-${shortName}-002`,
        severity: "medium",
        category: "生命周期衰退",
        title: `${shortName}: 后段短会话（≤5条）比例从 ${first.shortSessionRate.toFixed(0)}% 升至 ${last.shortSessionRate.toFixed(0)}%`,
        description: "用户在后段更频繁地开启极短会话（可能是尝试后立即放弃），表明信任或耐心下降。",
        evidence: [
          `前20% 短会话: ${first.shortSessionRate.toFixed(0)}%`,
          `后20% 短会话: ${last.shortSessionRate.toFixed(0)}%`,
        ],
        affectedSessions: [],
        recommendation: "在后段会话中提供更积极的引导。主动询问'需要我解释当前项目结构吗？'来重建用户信任。",
        confidence: 0.55,
      });
    }

    // 用户消息长度变化
    const lenDelta = ((last.avgUserMsgLen - first.avgUserMsgLen) / (first.avgUserMsgLen || 1) * 100);
    if (lenDelta < -30 && last.avgUserMsgLen > 0) {
      reports.push({
        id: `LIFE-${shortName}-003`,
        severity: "medium",
        category: "生命周期退行",
        title: `${shortName}: 用户消息长度从 ${first.avgUserMsgLen.toFixed(0)}字 降至 ${last.avgUserMsgLen.toFixed(0)}字（${lenDelta.toFixed(0)}%）`,
        description: "用户在后段输入越来越短，可能已默认 Agent 不需要详细指令（信任建立）或失去详细描述的意愿（耐心耗尽）。",
        evidence: [],
        affectedSessions: [],
        recommendation: "很难从长度直接推断正负含义，需结合错误率/短会话率综合判断。若三者同时恶化 → 用户流失预警。",
        confidence: 0.45,
      });
    }
  }

  return reports;
}

// ════════════════════════════════════════════════════════════════════
//  入口
// ════════════════════════════════════════════════════════════════════

const isMain = process.argv[1]?.endsWith("user-insight-experiment.ts") ||
  process.argv[1]?.endsWith("user-insight-experiment");

if (isMain) {
  const loader = new DataLoader();
  try {
    const reports = runUserInsightExperiment(loader);

    printHeader("洞察摘要 v2");
    printMetric("发现洞察数", reports.length);

    const severityOrder: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };
    const sorted = [...reports].sort((a, b) => severityOrder[a.severity] - severityOrder[b.severity]);

    for (const r of sorted) {
      printFinding(r.severity, `[${r.id}] ${r.title}`, r.description);
      console.log(chalk.gray(`          建议: ${r.recommendation}`));
      console.log(chalk.gray(`          置信度: ${(r.confidence * 100).toFixed(0)}% | 影响会话: ${r.affectedSessions.length}`));
      if (r.evidence.length > 0) {
        console.log(chalk.gray(`          证据: ${r.evidence.slice(0, 3).join(" | ")}`));
      }
      console.log();
    }

    console.log(chalk.bold.cyan(`${"═".repeat(80)}`));
    console.log(chalk.bold.cyan("  实验完成。以上洞察基于非 compacted 会话 + 统计检验，结合人工验证效果更佳。"));
    console.log(chalk.bold.cyan(`${"═".repeat(80)}\n`));

  } finally {
    loader.close();
  }
}
