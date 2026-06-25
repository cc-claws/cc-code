#!/usr/bin/env bun
/**
 * 单 trace 的 system prompt 段落拆解
 *
 * 用法: bun .claude/skills/langfuse/scripts/prompt-breakdown.ts <traceId>
 */
import { api, fetchObservations, fmt, pct } from "./lib.ts";

const traceId = process.argv[2];
if (!traceId) {
  console.error("Usage: bun prompt-breakdown.ts <traceId>");
  process.exit(1);
}

const [trace, observations] = await Promise.all([
  api(`/api/public/traces/${traceId}`),
  fetchObservations(traceId),
]);

const generations = observations.filter((o: any) => o.type === "GENERATION");
if (!generations.length) { console.log("No LLM generations found."); process.exit(0); }

console.log(`## Trace: "${(trace.input as string)?.slice(0, 60)}"\n`);

const gen = generations[0];
const input = gen.input;
let messages: any[] = [];
if (typeof input === "string") {
  try { messages = JSON.parse(input).messages || []; } catch {}
} else if (input && typeof input === "object") {
  messages = (input as any).messages || [];
}

interface Section {
  label: string;
  role: string;
  chars: number;
  preview: string;
}

const sections: Section[] = [];
let totalChars = 0;

for (const m of messages) {
  const role = m.role || "?";
  const content = m.content ?? "";
  const text = typeof content === "string" ? content : JSON.stringify(content);
  const chars = text.length;

  let label = role;
  if (role === "system") {
    const first = text.split("\n")[0].slice(0, 80);
    if (first.includes("CLAUDE.md")) label = "CLAUDE.md";
    else if (first.includes("Deferred Tools") || first.includes("ExtraTools")) label = "Deferred Tools";
    else if (first.toLowerCase().includes("skill")) label = "Skills Summary";
    else if (first.includes("SubAgent") || first.includes("Subagent")) label = "SubAgent Defs";
    else if (first.includes("You are an interactive")) label = "System Prompt (core)";
    else label = first.slice(0, 50);
  } else if (role === "user") {
    label = text.slice(0, 50).replace(/\n/g, " ");
  } else if (role === "assistant") {
    label = text.slice(0, 50).replace(/\n/g, " ");
  } else if (role === "tool") {
    label = text.slice(0, 50).replace(/\n/g, " ");
  }

  sections.push({ label, role, chars, preview: text.replace(/\n/g, " ").slice(0, 100) });
  totalChars += chars;
}

// --- By Role ---
console.log("### By Role\n");
const roleGroups: Record<string, { count: number; chars: number }> = {};
for (const s of sections) {
  if (!roleGroups[s.role]) roleGroups[s.role] = { count: 0, chars: 0 };
  roleGroups[s.role].count++;
  roleGroups[s.role].chars += s.chars;
}

console.log("| Role | Count | Chars | % of Total |");
console.log("|------|-------|-------|------------|");
for (const [role, g] of Object.entries(roleGroups).sort((a, b) => b[1].chars - a[1].chars)) {
  console.log(`| ${role} | ${g.count} | ${fmt(g.chars)} | ${pct(g.chars, totalChars)} |`);
}
console.log(`| **Total** | **${sections.length}** | **${fmt(totalChars)}** | |`);

// --- System sections detail ---
const sysSections = sections.filter((s) => s.role === "system");
if (sysSections.length > 0) {
  const sysTotal = sysSections.reduce((a, s) => a + s.chars, 0);
  console.log(`\n### System Prompt Sections (${fmt(sysTotal)} chars, ${pct(sysTotal, totalChars)} of total)\n`);
  console.log("| Section | Chars | % of System | % of Total | Preview |");
  console.log("|---------|-------|-------------|------------|---------|");
  for (const s of sysSections) {
    console.log(
      `| ${s.label} | ${fmt(s.chars)} | ${pct(s.chars, sysTotal)} | ${pct(s.chars, totalChars)} | ${s.preview.slice(0, 60)} |`
    );
  }
}

// --- Top 10 non-system ---
const nonSys = sections.filter((s) => s.role !== "system").sort((a, b) => b.chars - a.chars);
if (nonSys.length > 0) {
  console.log(`\n### Top 10 Largest Non-System Messages\n`);
  console.log("| # | Role | Chars | % of Total | Preview |");
  console.log("|---|------|-------|------------|---------|");
  for (let i = 0; i < Math.min(10, nonSys.length); i++) {
    const s = nonSys[i];
    console.log(`| ${i + 1} | ${s.role} | ${fmt(s.chars)} | ${pct(s.chars, totalChars)} | ${s.preview.slice(0, 60)} |`);
  }
}
