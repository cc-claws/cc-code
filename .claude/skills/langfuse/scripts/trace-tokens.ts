#!/usr/bin/env bun
/**
 * 单 trace 逐轮 token 流 + 缓存异常检测
 *
 * 用法: bun .claude/skills/langfuse/scripts/trace-tokens.ts <traceId>
 */
import { api, fetchObservations, fmt, pct, genTokens } from "./lib.ts";

const traceId = process.argv[2];
if (!traceId) {
  console.error("Usage: bun trace-tokens.ts <traceId>");
  process.exit(1);
}

const [trace, observations] = await Promise.all([
  api(`/api/public/traces/${traceId}`),
  fetchObservations(traceId),
]);

const generations = observations.filter((o: any) => o.type === "GENERATION");
if (!generations.length) {
  console.log("No LLM generations found.");
  process.exit(0);
}

console.log(`## Trace: "${(trace.input as string)?.slice(0, 60)}"`);
console.log(`   Latency: ${trace.latency}s | Generations: ${generations.length}\n`);

// --- Token flow ---
console.log("### Token Flow\n");
console.log("| # | Input | Cache Read | Cache Create | Eff. New | Output | Cache% | Cumul. New |");
console.log("|---|-------|------------|--------------|----------|--------|--------|------------|");

let cumulativeNew = 0;
let totalInput = 0, totalCacheRead = 0, totalCacheCreate = 0, totalOutput = 0;
const roundTokens: { idx: number; input: number; cacheRead: number; cacheCreate: number; output: number; effective: number }[] = [];

for (let i = 0; i < generations.length; i++) {
  const tk = genTokens(generations[i]);
  const effective = tk.input - tk.cacheRead;
  cumulativeNew += effective;
  totalInput += tk.input;
  totalCacheRead += tk.cacheRead;
  totalCacheCreate += tk.cacheCreate;
  totalOutput += tk.output;

  const cachePct = tk.input > 0 ? ((tk.cacheRead / tk.input) * 100).toFixed(1) : "0";
  console.log(
    `| ${i + 1} | ${fmt(tk.input)} | ${fmt(tk.cacheRead)} | ${fmt(tk.cacheCreate)} | ${fmt(effective)} | ${fmt(tk.output)} | ${cachePct}% | ${fmt(cumulativeNew)} |`
  );
  roundTokens.push({ idx: i, ...tk, effective });
}

console.log(
  `\n**Totals**: Input=${fmt(totalInput)} CacheRead=${fmt(totalCacheRead)} (${pct(totalCacheRead, totalInput)}) CacheCreate=${fmt(totalCacheCreate)} Output=${fmt(totalOutput)}`
);

// --- Cache anomalies ---
console.log("\n### Cache Anomalies\n");
let anomalies = 0;

for (let i = 0; i < roundTokens.length; i++) {
  const r = roundTokens[i];

  // cache hit drop
  if (i > 0) {
    const prev = roundTokens[i - 1];
    const prevPct = prev.input > 0 ? (prev.cacheRead / prev.input) * 100 : 100;
    const curPct = r.input > 0 ? (r.cacheRead / r.input) * 100 : 100;
    if (curPct < prevPct - 10) {
      console.log(`  ⚠️ Round ${i + 1}: Cache hit dropped ${prevPct.toFixed(1)}% → ${curPct.toFixed(1)}% (${(prevPct - curPct).toFixed(0)}pp)`);
      anomalies++;
    }
  }

  // cache creation
  if (r.cacheCreate > 0) {
    console.log(`  📦 Round ${i + 1}: Cache creation = ${fmt(r.cacheCreate)} tokens (new prefix cached)`);
    anomalies++;
  }

  // high effective new
  if (r.effective > 5000) {
    console.log(`  🔴 Round ${i + 1}: Effective new = ${fmt(r.effective)} (>5K — check tool results or context injection)`);
    anomalies++;
  }

  // high latency
  const g = generations[i];
  if (g.latency > 60) {
    console.log(`  🐌 Round ${i + 1}: Latency = ${g.latency.toFixed(1)}s (>60s)`);
    anomalies++;
  }
}

// input tokens decreasing = context truncation / compact
for (let i = 1; i < roundTokens.length; i++) {
  const prev = roundTokens[i - 1];
  const curr = roundTokens[i];
  if (curr.input < prev.input * 0.85) {
    console.log(`  ✂️ Round ${i + 1}: Input dropped ${fmt(prev.input)} → ${fmt(curr.input)} (possible compact/truncation)`);
    anomalies++;
  }
}

if (anomalies === 0) {
  console.log("  ✅ No anomalies detected.");
}
