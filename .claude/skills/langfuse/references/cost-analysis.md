# Cost Analysis & Reflection

When a trace or task shows high token consumption, use the analysis script and then reflect on optimization opportunities.

## Analysis Script

`.claude/skills/langfuse/scripts/analyze.ts` — unified analysis with multiple modes.

```bash
bun .claude/skills/langfuse/scripts/analyze.ts [N]           # Overview + trace table + flags
bun .claude/skills/langfuse/scripts/analyze.ts --tools [N]   # Tool call analysis
bun .claude/skills/langfuse/scripts/analyze.ts --growth [N]  # Context growth trend
bun .claude/skills/langfuse/scripts/analyze.ts --report [N]  # Full report (all dimensions)
bun .claude/skills/langfuse/scripts/analyze.ts --trace-id <id>  # Single trace detail
```

### Report Sections

| # | Section | What it shows |
|---|---------|---------------|
| 1 | Overview | Aggregate stats, cache efficiency, output/input ratio |
| 2 | Per-Trace Table | Input/output/cache/latency per trace |
| 3 | Tool Analysis | Frequency, avg latency, redundancy detection, tool→context growth per step |
| 4 | Context Growth | Per-trace token trend (visual bar chart), session accumulation, cross-trace growth rate |
| 5 | System Prompt Occupancy | Section breakdown with estimated tokens, system vs conversation ratio |
| 6 | Most Expensive Trace | Per-LLM-call detail with delta |
| 7 | Summary & Flags | Auto-detected issues (low cache, redundant tools, slow calls, etc.) |

## Reflection Protocol

After running the analysis, if you observe **any** of the following patterns, proactively suggest optimizations:

### Red Flags

| Pattern | Threshold | Root Cause |
|---------|-----------|------------|
| Cache hit rate < 90% | Single trace | System prompt instability, cold start, or prompt structure changing across turns |
| Effective new tokens > 20K per trace | Single trace | Tool results or conversation context growing unbounded |
| Output/Input ratio > 5% | Single trace | Model over-explaining; consider requesting more concise responses |
| Output/Input ratio < 0.1% | Single trace | Massive input for tiny output — likely unnecessary context loaded |
| LLM calls > 10 for simple task | Single trace | Agent looping or retrying; check if tools are failing |
| Single LLM call latency > 60s | Per-call | Model generating too much or complex reasoning for a simple task |

### Optimization Checklist

When reflecting, evaluate these dimensions:

#### 1. System Prompt Weight
- What % of total context is system prompt? (> 40% → consider trimming)
- Which section is the largest? Can it be shortened or loaded on-demand?
- CLAUDE.md: are all TRAPs/INFOs still relevant? Can stale entries be archived?
- Skills summary: can inactive skills be excluded or lazy-loaded?

#### 2. Context Accumulation
- Are tool results being retained across turns unnecessarily?
- Is micro-compact triggering at the right threshold (currently 0.70)?
- Are there redundant messages (e.g., same file read multiple times)?

#### 3. Agent Loop Efficiency
- Is the agent making redundant tool calls (reading same file, re-running same command)?
- Could multiple sequential reads be replaced with a single batch read?
- Is the agent exploring broadly when a targeted search would suffice?

#### 4. Task Decomposition
- Could a complex multi-step task be broken into focused sub-tasks with less context per step?
- Are sub-agents being used where they'd reduce the parent's context window pressure?

### Reflection Output Format

When presenting reflection findings, use this structure:

```
## Cost Reflection

### Metrics
- Traces analyzed: N
- Total input: X tokens (Y% cache hit)
- Total output: Z tokens
- Avg LLM calls per trace: M

### Findings
1. [Pattern observed with specific trace example]
2. [Another pattern]

### Recommendations
1. [Specific, actionable optimization] — estimated savings: ~X tokens/trace
2. [Another recommendation]
```
