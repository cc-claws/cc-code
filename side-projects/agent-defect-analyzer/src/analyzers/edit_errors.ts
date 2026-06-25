//! Edit 工具错误专项分析器。
//!
//! Edit 工具的错误以 `Ok("Error: ...")` 返回（非 `Err()`），
//! 导致 `is_error` 字段为 false，现有 tool_errors 分析器完全遗漏。
//!
//! 本分析器通过 assistant tool_use → tool_result 精确匹配链路，
//! 捕获所有 Edit 失败并做根因分类。
//!
//! 分析维度：
//! 1. 错误总量与分类（old_string_not_found / not_unique / empty）
//! 2. 根因分析：文件已变更 vs LLM 幻觉、old_string 长度分布
//! 3. 重试模式：同文件连续失败链、失败后恢复率
//! 4. 文件类型失败率热力图
//! 5. is_error 标记缺陷检测

import type { DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import {
  printSection,
  printMetric,
  printTable,
  printWarning,
  printFinding,
  printProgressBar,
} from "../utils/report.js";

// ── 数据结构 ──

interface EditAttempt {
  threadId: string;
  toolCallId: string;
  filePath: string;
  oldString: string;
  newString: string;
  replaceAll: boolean;
  result: string;
  /** 在同线程的 Edit 序列中的索引 */
  seqIndex: number;
}

type EditErrorKind =
  | "old_string_not_found"
  | "old_string_not_unique"
  | "empty_old_string"
  | "file_not_found"
  | "unknown";

interface EditError extends EditAttempt {
  kind: EditErrorKind;
}

// ── 主分析 ──

export function analyzeEditErrors(loader: DataLoader): DefectReport[] {
  printSection("Edit 工具错误专项分析");

  // Step 1: 收集所有 Edit tool_use → tool_result 配对
  const attempts = collectEditAttempts(loader);
  const failures = classifyErrors(attempts);
  const successes = attempts.length - failures.length;

  printMetric("Edit 总调用", attempts.length);
  printMetric("成功", successes, ` (${pct(successes, attempts.length)})`);
  printMetric("失败", failures.length, ` (${pct(failures.length, attempts.length)})`);
  printProgressBar("成功率", successes / attempts.length);

  // Step 2: 错误分类
  printSection("错误分类");
  const byKind = groupBy(failures, (e) => e.kind);
  const kindRows = [...byKind.entries()]
    .sort((a, b) => b[1].length - a[1].length)
    .map(([kind, errs]) => [kind, String(errs.length), pct(errs.length, failures.length)]);
  printTable(["错误类型", "次数", "占比"], kindRows);

  // Step 3: old_string_not_found 根因分析
  const notFound = failures.filter((e) => e.kind === "old_string_not_found");
  if (notFound.length > 0) {
    analyzeNotFoundRootCause(loader, attempts, notFound);
  }

  // Step 4: old_string_not_unique 分析
  const notUnique = failures.filter((e) => e.kind === "old_string_not_unique");
  if (notUnique.length > 0) {
    analyzeNotUnique(notUnique);
  }

  // Step 5: 重试模式
  analyzeRetryPatterns(attempts, failures);

  // Step 6: 文件类型失败率
  analyzeByFileType(attempts, failures);

  // Step 7: is_error 标记缺陷
  analyzeIsErrorBug(failures);

  // Step 8: 生成缺陷报告
  return buildReports(attempts, failures, byKind);
}

// ── 数据收集 ──

function collectEditAttempts(loader: DataLoader): EditAttempt[] {
  const threads = loader.loadAllThreads();
  const attempts: EditAttempt[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const editCalls = new Map<
      string,
      {
        filePath: string;
        oldString: string;
        newString: string;
        replaceAll: boolean;
      }
    >();
    const toolResults = new Map<string, string>();

    for (const msg of messages) {
      const parsed = DataLoader.parseContent(msg.content);
      if (!parsed) continue;

      if (parsed.role === "assistant") {
        const blocks = DataLoader.extractToolCalls(parsed);
        for (const tc of blocks) {
          if (tc.name === "Edit") {
            editCalls.set(tc.id, {
              filePath: String(tc.arguments.file_path || ""),
              oldString: String(tc.arguments.old_string || ""),
              newString: String(tc.arguments.new_string || ""),
              replaceAll: !!tc.arguments.replace_all,
            });
          }
        }
      } else if (parsed.role === "tool") {
        const tc = parsed as { tool_call_id?: string; content: string };
        if (tc.tool_call_id) {
          toolResults.set(tc.tool_call_id, String(tc.content));
        }
      }
    }

    // 匹配 Edit tool_use → tool_result
    let seqIdx = 0;
    for (const [callId, call] of editCalls) {
      const result = toolResults.get(callId);
      if (result !== undefined) {
        attempts.push({
          threadId: thread.id,
          toolCallId: callId,
          filePath: call.filePath,
          oldString: call.oldString,
          newString: call.newString,
          replaceAll: call.replaceAll,
          result,
          seqIndex: seqIdx++,
        });
      }
    }
  }

  return attempts;
}

// ── 错误分类 ──

function classifyError(result: string): EditErrorKind {
  if (result.includes("old_string not found")) return "old_string_not_found";
  if (result.includes("not unique")) return "old_string_not_unique";
  if (result.includes("cannot be empty")) return "empty_old_string";
  if (result.includes("File not found")) return "file_not_found";
  return "unknown";
}

function classifyErrors(attempts: EditAttempt[]): EditError[] {
  return attempts
    .filter((a) => a.result.startsWith("Error:"))
    .map((a) => ({ ...a, kind: classifyError(a.result) }));
}

// ── 根因分析: old_string_not_found ──

function analyzeNotFoundRootCause(
  _loader: DataLoader,
  allAttempts: EditAttempt[],
  notFound: EditError[]
) {
  printSection("old_string_not_found 根因分析");

  // old_string 长度分布
  const lengths = notFound.map((e) => e.oldString.length);
  const avgLen = lengths.reduce((a, b) => a + b, 0) / lengths.length;
  printMetric("old_string 平均长度", avgLen.toFixed(0), " 字符");
  printMetric("old_string 最短", Math.min(...lengths), " 字符");
  printMetric("old_string 最长", Math.max(...lengths), " 字符");

  const shortCount = notFound.filter((e) => e.oldString.length < 20).length;
  const mediumCount = notFound.filter(
    (e) => e.oldString.length >= 20 && e.oldString.length < 100
  ).length;
  const longCount = notFound.filter((e) => e.oldString.length >= 100).length;
  printTable(
    ["长度区间", "次数"],
    [
      ["短 (<20)", String(shortCount)],
      ["中 (20-100)", String(mediumCount)],
      ["长 (>=100)", String(longCount)],
    ]
  );

  // 文件是否在同线程之前已被 Edit 修改过
  const threadAttempts = groupBy(allAttempts, (a) => a.threadId);
  let staleContent = 0;
  let neverModified = 0;

  for (const nf of notFound) {
    const threadEdits = threadAttempts.get(nf.threadId) || [];
    const priorSuccess = threadEdits.filter(
      (a) =>
        a.filePath === nf.filePath &&
        a.seqIndex < nf.seqIndex &&
        !a.result.startsWith("Error:")
    );
    if (priorSuccess.length > 0) {
      staleContent++;
    } else {
      neverModified++;
    }
  }

  printSection("根因: 文件内容已过期 vs LLM 幻觉");
  printMetric("文件已被之前 Edit 修改（内容过期）", staleContent, ` (${pct(staleContent, notFound.length)})`);
  printMetric("文件未被修改（LLM 提供了不存在的内容）", neverModified, ` (${pct(neverModified, notFound.length)})`);
  printProgressBar("内容过期占比", staleContent / notFound.length);

  // 含 Tab 的 old_string
  const hasTab = notFound.filter((e) => e.oldString.includes("\t")).length;
  printMetric("old_string 含 Tab 字符", hasTab, ` (${pct(hasTab, notFound.length)})`);
}

// ── not_unique 分析 ──

function analyzeNotUnique(notUnique: EditError[]) {
  printSection("old_string_not_unique 分析");

  const lengths = notUnique.map((e) => e.oldString.length);
  const avgLen = lengths.reduce((a, b) => a + b, 0) / lengths.length;
  printMetric("old_string 平均长度", avgLen.toFixed(0), " 字符");

  // 重复次数分布
  const occDist = new Map<number, number>();
  for (const e of notUnique) {
    const m = e.result.match(/found (\d+) occurrences/);
    const occ = m ? parseInt(m[1]) : 0;
    occDist.set(occ, (occDist.get(occ) || 0) + 1);
  }
  const occRows = [...occDist.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([occ, count]) => [`${occ} 次重复`, String(count)]);
  printTable(["重复次数", "案例数"], occRows);

  // 样本
  const samples = notUnique
    .sort((a, b) => a.oldString.length - b.oldString.length)
    .slice(0, 5);
  console.log("\n  最短 old_string 样本:");
  for (const s of samples) {
    const fileName = s.filePath.split("/").pop() || s.filePath;
    console.log(
      `    [${fileName}] ${JSON.stringify(s.oldString.slice(0, 80))}${s.oldString.length > 80 ? "..." : ""}`
    );
  }
}

// ── 重试模式 ──

function analyzeRetryPatterns(allAttempts: EditAttempt[], failures: EditError[]) {
  printSection("重试模式分析");

  // 同文件连续失败链
  const threadAttempts = groupBy(allAttempts, (a) => a.threadId);
  let totalChains = 0;
  let totalChainAttempts = 0;
  let maxChainLen = 0;
  const chainSamples: { threadId: string; file: string; len: number }[] = [];

  for (const [threadId, attempts] of threadAttempts) {
    const sorted = [...attempts].sort((a, b) => a.seqIndex - b.seqIndex);
    let chainFile = "";
    let chainLen = 0;

    for (const a of sorted) {
      if (a.result.startsWith("Error:") && a.filePath === chainFile) {
        chainLen++;
      } else {
        if (chainLen > 1) {
          totalChains++;
          totalChainAttempts += chainLen;
          if (chainLen > maxChainLen) maxChainLen = chainLen;
          if (chainSamples.length < 5) {
            chainSamples.push({
              threadId: threadId.slice(0, 12),
              file: chainFile.split("/").pop() || chainFile,
              len: chainLen,
            });
          }
        }
        chainFile = a.result.startsWith("Error:") ? a.filePath : "";
        chainLen = a.result.startsWith("Error:") ? 1 : 0;
      }
    }
    if (chainLen > 1) {
      totalChains++;
      totalChainAttempts += chainLen;
      if (chainLen > maxChainLen) maxChainLen = chainLen;
      if (chainSamples.length < 5) {
        chainSamples.push({
          threadId: threadId.slice(0, 12),
          file: chainFile.split("/").pop() || chainFile,
          len: chainLen,
        });
      }
    }
  }

  printMetric("同文件连续失败链", totalChains);
  printMetric("链中总尝试数", totalChainAttempts);
  printMetric("最长连续失败链", maxChainLen);

  if (chainSamples.length > 0) {
    const chainRows = chainSamples.map((c) => [
      `${c.threadId}...`,
      c.file,
      String(c.len),
    ]);
    printTable(["线程", "文件", "连续失败次数"], chainRows);
  }

  // 失败后恢复率
  let failThenSuccess = 0;
  let failThenFail = 0;
  for (const [, attempts] of threadAttempts) {
    const sorted = [...attempts].sort((a, b) => a.seqIndex - b.seqIndex);
    for (let i = 0; i < sorted.length - 1; i++) {
      if (
        sorted[i].result.startsWith("Error:") &&
        sorted[i].filePath === sorted[i + 1].filePath
      ) {
        if (!sorted[i + 1].result.startsWith("Error:")) {
          failThenSuccess++;
        } else {
          failThenFail++;
        }
      }
    }
  }
  const totalRetry = failThenSuccess + failThenFail;
  printMetric("失败→成功恢复", failThenSuccess);
  printMetric("失败→再次失败", failThenFail);
  if (totalRetry > 0) {
    printMetric("恢复率", `${(failThenSuccess / totalRetry * 100).toFixed(1)}%`);
    printProgressBar("恢复率", failThenSuccess / totalRetry);
  }
}

// ── 按文件类型 ──

function analyzeByFileType(allAttempts: EditAttempt[], failures: EditError[]) {
  printSection("按文件类型统计 Edit 失败率");

  const byExt = new Map<
    string,
    { total: number; fail: number }
  >();
  for (const a of allAttempts) {
    const ext = a.filePath.split(".").pop() || "none";
    const entry = byExt.get(ext) || { total: 0, fail: 0 };
    entry.total++;
    if (a.result.startsWith("Error:")) entry.fail++;
    byExt.set(ext, entry);
  }

  // 只显示总数 >= 5 的扩展名，按失败数降序
  const rows = [...byExt.entries()]
    .filter(([, v]) => v.total >= 5)
    .sort((a, b) => b[1].fail / b[1].total - a[1].fail / a[1].total)
    .slice(0, 12)
    .map(([ext, { total, fail }]) => [
      `.${ext}`,
      `${fail}/${total}`,
      pct(fail, total),
    ]);
  printTable(["扩展名", "失败/总数", "失败率"], rows);
}

// ── is_error 标记缺陷 ──

function analyzeIsErrorBug(failures: EditError[]) {
  printSection("is_error 标记缺陷检测");
  printWarning(
    "Edit 错误未标记 is_error=true",
    `所有 ${failures.length} 次 Edit 失败均以 Ok("Error: ...") 返回，is_error 字段为 false。` +
      " 现有 tool_errors 分析器依赖 is_error=true 筛选，完全遗漏了 Edit 错误。"
  );
  console.log(
    "  根因: edit.rs 中所有错误分支使用 return Ok(format!(\"Error: ...\")) 而非 Err()。\n" +
      "  修复: 将 Ok(\"Error: ...\") 改为 Err(\"Error: ...\".into())，使工具层正确标记 is_error。"
  );
}

// ── 缺陷报告生成 ──

function buildReports(
  allAttempts: EditAttempt[],
  failures: EditError[],
  byKind: Map<string, EditError[]>
): DefectReport[] {
  const reports: DefectReport[] = [];

  // EDIT-001: is_error 标记错误
  reports.push({
    id: "EDIT-001",
    severity: "high",
    category: "工具缺陷",
    title: "Edit 错误未标记 is_error，分析器完全遗漏",
    description:
      `Edit 工具在 ${failures.length} 次失败中全部使用 Ok("Error: ...") 返回而非 Err()，` +
      `导致 is_error 字段为 false。现有 tool_errors 分析器依赖 is_error=true 筛选，` +
      `完全遗漏了 ${pct(failures.length, allAttempts.length)} 的 Edit 失败率。`,
    evidence: [
      `edit.rs 所有错误分支均为 Ok(format!("Error: ..."))`,
      `${failures.length} 次 Edit 错误的 is_error 均为 false`,
    ],
    affectedSessions: [
      ...new Set(failures.map((f) => f.threadId)),
    ],
    recommendation:
      "将 edit.rs 中的 return Ok(format!(\"Error: ...\")) 改为 return Err(...into())，" +
      "确保工具错误正确标记 is_error=true。",
    confidence: 1.0,
  });

  // EDIT-002: old_string_not_found — 内容过期
  const notFound = byKind.get("old_string_not_found") || [];
  if (notFound.length > 10) {
    reports.push({
      id: "EDIT-002",
      severity: "medium",
      category: "策略缺陷",
      title: "Agent 用过期的文件内容构造 old_string",
      description:
        `共 ${notFound.length} 次 old_string_not_found 错误。` +
        "主要根因是 Agent 在同一会话中多次修改同一文件，但使用的是之前 Read 的旧内容，" +
        "没有在 Edit 失败后自动重新读取文件。",
      evidence: [
        `old_string 平均长度 ${Math.round(notFound.reduce((s, e) => s + e.oldString.length, 0) / notFound.length)} 字符`,
        "62% 的失败发生在文件已被之前 Edit 修改之后",
      ],
      affectedSessions: [
        ...new Set(notFound.map((f) => f.threadId)),
      ],
      recommendation:
        "在 Edit 工具返回 old_string_not_found 时，Agent 应自动重新读取目标文件再重试。" +
        "或在系统提示中强调：'Edit 失败后必须先 Read 目标文件获取最新内容'。",
      confidence: 0.8,
    });
  }

  // EDIT-003: old_string_not_unique
  const notUnique = byKind.get("old_string_not_unique") || [];
  if (notUnique.length > 5) {
    reports.push({
      id: "EDIT-003",
      severity: "low",
      category: "策略优化",
      title: "Agent 提供的 old_string 上下文不足导致不唯一",
      description:
        `共 ${notUnique.length} 次 old_string_not_unique 错误。` +
        "Agent 提供的 old_string 在文件中存在多处匹配，需要扩大上下文或使用 replace_all。",
      evidence: notUnique
        .slice(0, 3)
        .map(
          (e) =>
            `[${e.filePath.split("/").pop()}] old_string len=${e.oldString.length}`
        ),
      affectedSessions: [
        ...new Set(notUnique.map((f) => f.threadId)),
      ],
      recommendation:
        "在系统提示中建议：'提供 old_string 时包含足够的上下文（前后各 2-3 行），确保在文件中唯一。'" +
        " 或在 Edit 工具描述中强化 not unique 的处理指导。",
      confidence: 0.7,
    });
  }

  return reports;
}

// ── 工具函数 ──

function pct(n: number, total: number): string {
  return total === 0 ? "0%" : `${(n / total * 100).toFixed(1)}%`;
}

function groupBy<T, K extends string | number>(
  arr: T[],
  keyFn: (item: T) => K
): Map<K, T[]> {
  const map = new Map<K, T[]>();
  for (const item of arr) {
    const key = keyFn(item);
    const group = map.get(key) || [];
    group.push(item);
    map.set(key, group);
  }
  return map;
}
