/// <reference types="@cloudflare/workers-types" />

import { DurableObject } from "cloudflare:workers";

interface PairInfo {
	code: string;
	senderWs: WebSocket | null;
	receiverWs: WebSocket | null;
	createdAt: number;
	used: boolean;
}

/**
 * PairRoom — Durable Object 单例，管理所有配对。
 * 通过 idFromName("global") 确保 sender/receiver 无论
 * 被路由到哪个 Worker isolate，都进入同一个 DO 实例。
 */
export class PairRoom extends DurableObject {
	private pairs = new Map<string, PairInfo>();
	private alarmScheduled = false;

	// ---- HTTP fetch: WebSocket 升级入口 ----

	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);
		const role = url.searchParams.get("role");
		const code = url.searchParams.get("code") || undefined;

		// 惰性清理过期配对
		this.cleanExpired();

		const pair = new WebSocketPair();
		const [client, server] = [pair[0], pair[1]];

		// 将 role/code 附加到 server socket 的 ctx tag 上
		this.ctx.acceptWebSocket(server, [
			`role:${role}`,
			...(code ? [`code:${code}`] : []),
		]);

		return new Response(null, {
			status: 101,
			webSocket: client,
		});
	}

	// ---- WebSocket 消息处理 ----

	async webSocketMessage(ws: WebSocket, message: string): Promise<void> {
		try {
			const tags = this.ctx.getTags(ws);
			const role = tags.find((t) => t.startsWith("role:"))?.slice(5);

			if (role === "sender") {
				// Sender 第一条消息：request_pair
				const msg = JSON.parse(message);
				if (msg.type === "request_pair") {
					const pairCode = this.generateCode();
					this.pairs.set(pairCode, {
						code: pairCode,
						senderWs: ws,
						receiverWs: null,
						createdAt: Date.now(),
						used: false,
					});
					ws.send(JSON.stringify({ type: "pair_created", pair_code: pairCode }));
					this.scheduleCleanup();
					return;
				}
			}

			if (role === "receiver") {
				const msg = JSON.parse(message);
				if (msg.type === "join_pair") {
					const pair = this.pairs.get(msg.pair_code);
					if (!pair || pair.used || Date.now() - pair.createdAt > 5 * 60 * 1000) {
						ws.send(JSON.stringify({
							type: "error",
							code: "PAIR_INVALID",
							message: "Pair code invalid or expired",
						}));
						ws.close();
						return;
					}
					pair.receiverWs = ws;
					pair.used = true;
					if (pair.senderWs) {
						pair.senderWs.send(JSON.stringify({ type: "pair_joined" }));
					}
					ws.send(JSON.stringify({ type: "pair_joined" }));
					return;
				}
			}
		} catch {
			// 非 JSON 消息（加密数据分片），直接走转发
		}

		// 默认：转发消息给配对另一方
		const pair = this.findByWs(ws);
		if (!pair) return;
		const tags = this.ctx.getTags(ws);
		const role = tags.find((t) => t.startsWith("role:"))?.slice(5);
		const target = role === "sender" ? pair.receiverWs : pair.senderWs;
		if (target) {
			target.send(message);
		}
	}

	async webSocketClose(
		ws: WebSocket,
		_code: number,
		_reason: string,
		_wasClean: boolean,
	): Promise<void> {
		this.cleanupDisconnected(ws);
	}

	async webSocketError(ws: WebSocket, _error: unknown): Promise<void> {
		this.cleanupDisconnected(ws);
	}

	// ---- Alarm 定时清理 ----

	async alarm(): Promise<void> {
		this.alarmScheduled = false;
		this.cleanExpired();
		if (this.pairs.size > 0) {
			await this.ctx.storage.setAlarm(Date.now() + 60_000);
			this.alarmScheduled = true;
		}
	}

	// ---- 内部方法 ----

	private generateCode(): string {
		let code: string;
		do {
			code = String(Math.floor(100000 + Math.random() * 900000));
		} while (this.pairs.has(code));
		return code;
	}

	private findByWs(ws: WebSocket): PairInfo | null {
		for (const pair of this.pairs.values()) {
			if (pair.senderWs === ws || pair.receiverWs === ws) return pair;
		}
		return null;
	}

	private cleanExpired(): void {
		const now = Date.now();
		for (const [code, pair] of this.pairs) {
			if (now - pair.createdAt > 5 * 60 * 1000) {
				if (pair.senderWs && !pair.used) {
					try { pair.senderWs.close(); } catch { /* ignore */ }
				}
				this.pairs.delete(code);
			}
		}
	}

	private cleanupDisconnected(ws: WebSocket): void {
		const tags = this.ctx.getTags(ws);
		const role = tags.find((t) => t.startsWith("role:"))?.slice(5);
		const pair = this.findByWs(ws);
		if (!pair) return;

		const other = role === "sender" ? pair.receiverWs : pair.senderWs;
		if (other) {
			try {
				other.send(JSON.stringify({
					type: "error",
					code: "PEER_DISCONNECTED",
					message: "Peer disconnected",
				}));
			} catch { /* ignore */ }
		}
		this.pairs.delete(pair.code);
	}

	private scheduleCleanup(): void {
		if (this.alarmScheduled) return;
		this.ctx.storage.setAlarm(Date.now() + 60_000);
		this.alarmScheduled = true;
	}
}
