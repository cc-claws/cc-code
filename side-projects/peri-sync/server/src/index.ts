import type { ServerWebSocket } from "bun";
import { PairManager } from "./pair-manager";

const pairManager = new PairManager();

interface WsCtx {
  role: "sender" | "receiver";
  code?: string;
}

const port = parseInt(process.env.PORT || "8080");
console.log(`Peri Sync Relay starting on port ${port}...`);

Bun.serve({
  port,
  websocket: {
    data: {} as WsCtx,

    open(ws: ServerWebSocket<WsCtx>): void {
      const role = ws.data.role;
      const code = ws.data.code;

      if (role === "sender") {
        const pairCode = pairManager.createPair(
          ws as unknown as ServerWebSocket<unknown>
        );
        ws.send(
          JSON.stringify({
            type: "pair_created",
            pair_code: pairCode,
          })
        );
        console.log(`[pair] sender 创建配对码 ${pairCode}`);
      } else if (role === "receiver" && code) {
        const pair = pairManager.validateAndJoin(
          code,
          ws as unknown as ServerWebSocket<unknown>
        );
        if (pair && pair.senderWs) {
          pair.senderWs.send(
            JSON.stringify({ type: "pair_joined" })
          );
          ws.send(JSON.stringify({ type: "pair_joined" }));
          console.log(`[pair] receiver 加入配对码 ${code}`);
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
            message: "role must be sender or receiver",
          })
        );
        ws.close();
      }
    },

    message(
      ws: ServerWebSocket<WsCtx>,
      raw: string | Buffer
    ): void {
      const pair = pairManager.findByWs(
        ws as unknown as ServerWebSocket<unknown>
      );
      if (!pair) return;
      const target =
        ws.data.role === "sender"
          ? pair.receiverWs
          : pair.senderWs;
      if (target) {
        target.send(
          typeof raw === "string" ? raw : raw.toString()
        );
      }
    },

    close(ws: ServerWebSocket<WsCtx>): void {
      const pair = pairManager.findByWs(
        ws as unknown as ServerWebSocket<unknown>
      );
      if (pair) {
        const other =
          ws.data.role === "sender"
            ? pair.receiverWs
            : pair.senderWs;
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
            /* ignore */
          }
        }
        pairManager.remove(pair.code);
        console.log(
          `[pair] 配对码 ${pair.code} 连接断开`
        );
      }
    },
  },

  fetch(req, server) {
    const url = new URL(req.url);

    // 健康检查
    if (url.pathname === "/health") {
      return new Response("ok");
    }

    // WebSocket 升级
    if (url.pathname === "/ws") {
      const role = url.searchParams.get("role");
      const code = url.searchParams.get("code") || undefined;

      if (server.upgrade(req, {
        data: { role, code } as WsCtx,
      })) {
        return; // 升级成功，不返回 Response
      }
      return new Response("WebSocket upgrade failed", { status: 400 });
    }

    return new Response("Not Found", { status: 404 });
  },
});
