/// <reference types="@cloudflare/workers-types" />

import { PairRoom } from "./pair-do";

export { PairRoom };

export default {
	async fetch(
		request: Request,
		env: { PAIR_ROOM: DurableObjectNamespace },
	): Promise<Response> {
		const url = new URL(request.url);

		if (url.pathname === "/health") {
			return new Response("ok");
		}

		if (url.pathname === "/ws") {
			const id = env.PAIR_ROOM.idFromName("global");
			const stub = env.PAIR_ROOM.get(id);
			return stub.fetch(request);
		}

		return new Response("Not Found", { status: 404 });
	},
};
