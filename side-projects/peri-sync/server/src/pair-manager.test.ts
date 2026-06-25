import { describe, test, expect, beforeEach } from "bun:test";
import { PairManager } from "./pair-manager";
import type { ServerWebSocket } from "bun";

function makeMockWs(): ServerWebSocket<unknown> {
  return {
    send: () => {},
    close: () => {},
    data: {},
  } as unknown as ServerWebSocket<unknown>;
}

describe("PairManager", () => {
  let manager: PairManager;

  beforeEach(() => {
    manager = new PairManager();
  });

  test("createPair 生成6位数字码", () => {
    const ws = makeMockWs();
    const code = manager.createPair(ws);
    const num = parseInt(code, 10);
    expect(num).toBeGreaterThanOrEqual(100000);
    expect(num).toBeLessThanOrEqual(999999);
  });

  test("validateAndJoin 有效码返回pairInfo", () => {
    const senderWs = makeMockWs();
    const receiverWs = makeMockWs();
    const code = manager.createPair(senderWs);
    const result = manager.validateAndJoin(code, receiverWs);
    expect(result).not.toBeNull();
    expect(result!.used).toBe(true);
    expect(result!.receiverWs).toBe(receiverWs);
  });

  test("validateAndJoin 无效码返回null", () => {
    const ws = makeMockWs();
    const result = manager.validateAndJoin("000000", ws);
    expect(result).toBeNull();
  });

  test("validateAndJoin 码已使用后返回null", () => {
    const senderWs = makeMockWs();
    const receiverWs1 = makeMockWs();
    const receiverWs2 = makeMockWs();
    const code = manager.createPair(senderWs);
    const first = manager.validateAndJoin(code, receiverWs1);
    expect(first).not.toBeNull();
    const second = manager.validateAndJoin(code, receiverWs2);
    expect(second).toBeNull();
  });

  test("cleanup 过期码被移除", () => {
    const ws = makeMockWs();
    // 通过反射插入一个 createdAt 为 6 分钟前的 pair
    const code = "123456";
    (manager as any).pairs.set(code, {
      code,
      senderWs: ws,
      receiverWs: null,
      createdAt: Date.now() - 6 * 60 * 1000 - 1,
      used: false,
    });
    expect((manager as any).pairs.has(code)).toBe(true);
    manager.cleanupForTest();
    expect((manager as any).pairs.has(code)).toBe(false);
  });
});
