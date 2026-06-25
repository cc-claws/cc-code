#!/usr/bin/env bun
/**
 * 单 trace 逐轮消息组成 + 新增/变更消息 diff
 *
 * 用法: bun .claude/skills/langfuse/scripts/trace-messages.ts <traceId> [--detail]
 */
import { api, fetchObservations, fmt } from "./lib.ts";

const args = process.argv.slice(2);
const traceId = args.find((a) => !a.startsWith("--"));
const showDetail = args.includes("--detail");

if (!traceId) {
  console.error("Usage: bun trace-messages.ts <traceId> [--detail]");
  process.exit(1);
}

interface MsgSummary {
  role: string;
  chars: number;
  preview: string;
}

function parseMessages(raw: any): MsgSummary[] {
  let messages: any[] = [];
  if (typeof raw === "string") {
    try { messages = JSON.parse(raw).messages || []; } catch { return []; }
  } else if (raw && typeof raw === "object") {
    messages = (raw as any).messages || [];
  }
  if (!Array.isArray(messages)) return [];

  return messages.map((m: any) => {
    const role = m.role || "?";
    const content = m.content ?? m.tool_calls ?? "";
    const text = typeof content === "string"
      ? content
      : Array.isArray(content)
        ? content.map((c: any) => {
            if (typeof c === "string") return c;
            if (c.type === "text") return c.text ?? "";
            if (c.type === "tool_use") return `[tool_use: ${c.name}]`;
            if (c.type === "tool_result") return `[tool_result: ${c.tool_use_id}]`;
            return `[${c.type}]`;
          }).join("\n")
        : JSON.stringify(content);
    return { role, chars: text.length, preview: text.replace(/\n/g, " ").slice(0, 120) };
  });
}

function diffMsgs(prev: MsgSummary[], curr: MsgSummary[]) {
  const newIdx: number[] = [];
  const changedIdx: number[] = [];
  for (let i = 0; i < curr.length; i++) {
    if (i >= prev.length) newIdx.push(i);
    else if (prev[i].chars !== curr[i].chars || prev[i].role !== curr[i].role) changedIdx.push(i);
  }
  return { newIdx, changedIdx };
}

const [trace, observations] = await Promise.all([
  api(`/api/public/traces/${traceId}`),
  fetchObservations(traceId),
]);

const generations = observations.filter((o: any) => o.type === "GENERATION");
if (!generations.length) { console.log("No LLM generations found."); process.exit(0); }

console.log(`## Trace: "${(trace.input as string)?.slice(0, 60)}"`);
console.log(`   Generations: ${generations.length}\n`);

// --- Message composition table ---
console.log("### Message Composition\n");
console.log("| # | Sys | User | Asst | Tool | Total | New | Δ chars |");
console.log("|---|-----|------|------|------|-------|-----|---------|");

const prevMsgs: MsgSummary[] = [];
const rounds: { idx: number; gen: any; messages: MsgSummary[]; newIdx: number[]; changedIdx: number[] }[] = [];

for (let i = 0; i < generations.length; i++) {
  const messages = parseMessages(generations[i].input);
  const { newIdx, changedIdx } = diffMsgs(prevMsgs, messages);
  rounds.push({ idx: i, gen: generations[i], messages, newIdx, changedIdx });

  const roles: Record<string, number> = { system: 0, user: 0, assistant: 0, tool: 0 };
  let newChars = 0;
  for (let j = 0; j < messages.length; j++) {
    const r = messages[j].role === "tool" ? "tool" : messages[j].role;
    roles[r] = (roles[r] || 0) + 1;
    if (newIdx.includes(j) || changedIdx.includes(j)) newChars += messages[j].chars;
  }

  console.log(
    `| ${i + 1} | ${roles.system} | ${roles.user} | ${roles.assistant} | ${roles.tool} | ${messages.length} | ${newIdx.length + changedIdx.length} | ${fmt(newChars)} |`
  );

  prevMsgs.length = 0;
  prevMsgs.push(...messages);
}

// --- System prompt stability ---
const firstSys = rounds[0].messages.filter((m) => m.role === "system").map((m) => m.chars);
let sysUnstable = false;
for (let i = 1; i < rounds.length; i++) {
  const curSys = rounds[i].messages.filter((m) => m.role === "system").map((m) => m.chars);
  if (JSON.stringify(firstSys) !== JSON.stringify(curSys)) {
    if (!sysUnstable) {
      console.log("\n### System Prompt Stability: ⚠️ UNSTABLE\n");
      sysUnstable = true;
    }
    console.log(`  Round ${i + 1}: changed [${firstSys.join(", ")}] → [${curSys.join(", ")}] chars`);
  }
}
if (!sysUnstable) {
  console.log("\n  ✅ System prompt stable across all rounds.");
}

// --- Detail mode ---
if (showDetail) {
  console.log("\n### Per-Round Message Detail\n");
  for (const r of rounds) {
    if (r.newIdx.length === 0 && r.changedIdx.length === 0) continue;

    console.log(`\n#### Round ${r.idx + 1} (${r.messages.length} msgs, ${r.newIdx.length + r.changedIdx.length} changed)`);
    for (const idx of [...r.changedIdx, ...r.newIdx]) {
      const m = r.messages[idx];
      const tag = r.changedIdx.includes(idx) ? "CHANGED" : "NEW";
      console.log(`  [${tag}] [${m.role}] (${fmt(m.chars)} chars)`);
      console.log(`    ${m.preview}`);
    }

    const out = r.gen.output;
    const outText = (typeof out === "string" ? out : JSON.stringify(out ?? "")).replace(/\n/g, " ").slice(0, 150);
    console.log(`  → Output: ${outText}`);
  }
}
