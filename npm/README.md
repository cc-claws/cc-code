# Peri

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 Claude Code 全家桶**

[![npm](https://img.shields.io/npm/v/@cc-claw/peri)](https://www.npmjs.com/package/@cc-claw/peri)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE)

## 安装

```bash
npm install -g @cc-claw/peri
```

## 升级

```bash
npm update -g @cc-claw/peri
```

## 快速开始

```bash
# 启动交互式 TUI
peri

# 直接给任务
peri "解释这个项目的目录结构"

# 指定模型
peri --model deepseek/deepseek-chat "重构这个函数"
```

## 支持的平台

| 平台 | 架构 |
|------|------|
| Linux | x86_64, aarch64, riscv64 |
| macOS | x86_64 (Intel), aarch64 (Apple Silicon) |
| Windows | x86_64 |

## 为什么选 Peri？

| 对比项 | 其他终端 Agent | Peri |
|--------|---------------|------|
| 运行时 | Node.js / Bun，动辄吃 1GB 内存 | Rust 原生，启动快，~50MB 内存 |
| 模型绑定 | 锁死一家 LLM | 随便换：Anthropic、OpenAI 兼容、DeepSeek、GLM |
| Prompt 缓存 | 每轮重算，token 白烧 | 冻结 system prompt，95-99% 缓存命中率 |
| Claude Code 生态 | 不兼容 | 直接用 `.claude/` 配置、agents、skills、hooks、MCP |

## 链接

- [GitHub](https://github.com/wismyzhizi2018/peri)
- [Issues](https://github.com/wismyzhizi2018/peri/issues)
- [English README](https://github.com/wismyzhizi2018/peri#readme)
