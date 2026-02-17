import { Option, type bool } from "../std/mem";
import { Asyncify } from "../std/sync";

export class WebSocketConnection {
	private ws: Option<WebSocket>;
	private queue: Asyncify<string | ArrayBuffer | null>;

	private constructor(ws: Option<WebSocket>) {
		this.queue = new Asyncify(true);
		this.ws = ws;
		if (this.ws.is_some()) {
			this.ws.deref().addEventListener("message", async v => {
				if (typeof v.data == "string") {
					this.queue.resolve(v.data);
				} else if (v.data instanceof Blob) {
					this.queue.resolve((await v.data.bytes()).buffer);
				} else if (v.data instanceof ArrayBuffer) {
					this.queue.resolve(v.data);
				} else {
					this.invalidate();
					throw new Error("invalid message received");
				}
			});
		}
	}

	public static connect(target: string): Promise<WebSocketConnection> {
		const sock = new WebSocket(target);
		const wrapper = new WebSocketConnection(Option.some(sock));
		return new Promise(r => {
			sock.addEventListener("open", () => {
				r(wrapper);
			});
			sock.addEventListener("error", () => {
				wrapper.invalidate();
				r(wrapper);
			});
			sock.addEventListener("close", () => {
				wrapper.invalidate();
				r(wrapper);
			})
		});
	}

	private invalidate() {
		this.ws = Option.none();
		while (this.queue.has_pollers()) {
			this.queue.resolve(null);
		}
	}

	public async recv(): Promise<string | ArrayBuffer | null> {
		if (!this.ws.is_some() && !this.queue.has_backlog()) { return null; }
		return this.queue.poll();
	}

	public send(data: string | ArrayBuffer): bool {
		if (this.ws.is_some()) {
			this.ws.deref().send(data);
			return true;
		} else {
			return false;
		}
	}

	public close() {
		if (this.ws.is_some()) {
			this.ws.deref().close();
			this.invalidate();
		}
	}

	public is_ok(): bool {
		return this.ws.is_some();
	}
}

