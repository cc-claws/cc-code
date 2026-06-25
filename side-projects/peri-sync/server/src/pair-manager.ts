import type { ServerWebSocket } from "bun";

interface PairInfo {
  code: string;
  senderWs: ServerWebSocket<unknown> | null;
  receiverWs: ServerWebSocket<unknown> | null;
  createdAt: number;
  used: boolean;
}

export class PairManager {
  private pairs: Map<string, PairInfo> = new Map();
  private cleanupTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    this.startCleanup();
  }

  /** 生成 6 位随机配对码（100000-999999），保证不冲突 */
  private generateCode(): string {
    let code: string;
    do {
      code = String(Math.floor(100000 + Math.random() * 900000));
    } while (this.pairs.has(code));
    return code;
  }

  /** 创建新配对，存储到 Map，返回配对码 */
  createPair(senderWs: ServerWebSocket<unknown>): string {
    const code = this.generateCode();
    this.pairs.set(code, {
      code,
      senderWs,
      receiverWs: null,
      createdAt: Date.now(),
      used: false,
    });
    return code;
  }

  /** 校验配对码：存在且未过期且未使用时返回 pairInfo，并在失败时清理未配对项 */
  validateAndJoin(
    code: string,
    receiverWs: ServerWebSocket<unknown>
  ): PairInfo | null {
    const pair = this.pairs.get(code);
    if (!pair) return null;
    if (pair.used) return null;
    if (Date.now() - pair.createdAt > 5 * 60 * 1000) {
      this.pairs.delete(code);
      return null;
    }
    pair.receiverWs = receiverWs;
    pair.used = true;
    return pair;
  }

  /** 移除指定配对码 */
  remove(code: string): void {
    this.pairs.delete(code);
  }

  /** 根据 WebSocket 查找配对的另一方 */
  findByWs(ws: ServerWebSocket<unknown>): PairInfo | null {
    for (const pair of this.pairs.values()) {
      if (pair.senderWs === ws || pair.receiverWs === ws) {
        return pair;
      }
    }
    return null;
  }

  /** 获取配对码总数（供测试使用） */
  get size(): number {
    return this.pairs.size;
  }

  /** 暴露清理逻辑供测试使用 */
  cleanupForTest(): void {
    const now = Date.now();
    for (const [code, pair] of this.pairs) {
      if (now - pair.createdAt > 5 * 60 * 1000) {
        this.pairs.delete(code);
      }
    }
  }

  /** 启动 60 秒定时器，清理过期（>5分钟）的配对码 */
  private startCleanup(): void {
    this.cleanupTimer = setInterval(() => {
      const now = Date.now();
      for (const [code, pair] of this.pairs) {
        if (now - pair.createdAt > 5 * 60 * 1000) {
          this.pairs.delete(code);
        }
      }
    }, 60_000);
  }

  /** 停止定时器 */
  stop(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
  }
}
