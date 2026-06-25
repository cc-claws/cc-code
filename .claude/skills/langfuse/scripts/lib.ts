/**
 * Langfuse API 客户端 —— 供其他脚本 import
 *
 * 自动从 .env 读取 LANGFUSE_HOST / LANGFUSE_PUBLIC_KEY / LANGFUSE_SECRET_KEY
 */
const BASE_URL = (process.env.LANGFUSE_HOST || process.env.LANGFUSE_BASE_URL || "").replace(/\/$/, "");
const PUBLIC_KEY = process.env.LANGFUSE_PUBLIC_KEY || "";
const SECRET_KEY = process.env.LANGFUSE_SECRET_KEY || "";

if (!BASE_URL || !PUBLIC_KEY || !SECRET_KEY) {
  console.error("Missing LANGFUSE_HOST/PUBLIC_KEY/SECRET_KEY env vars");
  process.exit(1);
}

const authHeader = `Basic ${btoa(`${PUBLIC_KEY}:${SECRET_KEY}`)}`;

export async function api(path: string) {
  const res = await fetch(`${BASE_URL}${path}`, {
    headers: { Authorization: authHeader, "Content-Type": "application/json" },
  });
  if (!res.ok) throw new Error(`API ${path}: ${res.status} ${await res.text()}`);
  return res.json();
}

export async function fetchTraces(limit: number) {
  const data = await api(`/api/public/traces?limit=${limit}`);
  return (data.data || []) as any[];
}

export async function fetchObservations(traceId: string) {
  const all: any[] = [];
  let page = 1;
  while (true) {
    const data = await api(`/api/public/observations?traceId=${traceId}&limit=100&page=${page}`);
    const items = (data.data || []) as any[];
    all.push(...items);
    const meta = data.meta || {};
    if (page >= (meta.totalPages || 1)) break;
    page++;
  }
  return all;
}

export function fmt(n: number) {
  return n.toLocaleString();
}

export function pct(part: number, whole: number) {
  return whole > 0 ? `${((part / whole) * 100).toFixed(1)}%` : "-";
}

export function genTokens(g: any) {
  const u = g.usageDetails || (g.usage as any) || {};
  return {
    input: (u.input || u.prompt_tokens || 0) as number,
    output: (u.output || u.completion_tokens || 0) as number,
    cacheRead: (u.cache_read_input_tokens || 0) as number,
    cacheCreate: (u.cache_creation_input_tokens || 0) as number,
  };
}
