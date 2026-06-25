import type { ServerWebSocket } from "bun";
import { PairManager } from "./pair-manager";
import type { WsClientMessage, WsServerMessage } from "./types";

interface WsContext {
  role: "sender" | "receiver";
  code?: string;
}

export function createRelayHandler(pairManager: PairManager) {
  return {
    open(ws: ServerWebSocket<WsContext>): void {
      const { role, code } = ws.data;
      if (role === "sender") {
        const pairCode = pairManager.createPair(
          ws as unknown as ServerWebSocket<unknown>
        );
        ws.send(
          JSON.stringify({ type: "pair_created", pair_code: pairCode })
        );
      } else if (role === "receiver" && code) {
        const pair = pairManager.validateAndJoin(
          code,
          ws as unknown as ServerWebSocket<unknown>
        );
        if (pair && pair.senderWs) {
          pair.senderWs.send(JSON.stringify({ type: "pair_joined" }));
          ws.send(JSON.stringify({ type: "pair_joined" }));
        } else {
          ws.send(
            JSON.stringify({
              type: "error",
              code: "PAIR_INVALID",
              message: "无效或已过期的配对码",
            })
          );
          ws.close();
        }
      } else {
        ws.send(
          JSON.stringify({
            type: "error",
            code: "BAD_REQUEST",
            message: "缺少 role 或 code 参数",
          })
        );
        ws.close();
      }
    },

    message(
      ws: ServerWebSocket<WsContext>,
      raw: string | Buffer
    ): void {
      const pair = pairManager.findByWs(
        ws as unknown as ServerWebSocket<unknown>
      );
      if (!pair) return;
      const target =
        ws.data.role === "sender" ? pair.receiverWs : pair.senderWs;
      if (target) {
        target.send(typeof raw === "string" ? raw : raw.toString());
      }
    },

    close(ws: ServerWebSocket<WsContext>): void {
      const pair = pairManager.findByWs(
        ws as unknown as ServerWebSocket<unknown>
      );
      if (pair) {
        // 通知另一方连接已断开
        const other =
          ws.data.role === "sender" ? pair.receiverWs : pair.senderWs;
        if (other) {
          try {
            other.send(
              JSON.stringify({
                type: "error",
                code: "PEER_DISCONNECTED",
                message: "对方已断开连接",
              })
            );
          } catch {
            // 对方可能已关闭，忽略发送错误
          }
        }
        pairManager.remove(pair.code);
      }
    },
  };
}
