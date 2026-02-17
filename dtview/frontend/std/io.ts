import type { usize } from "./mem";

export interface Read {
	read_exact(length: usize): ArrayBuffer | null;
}

export interface Write {
	write(buf: ArrayBuffer): void;
}

export class BufReader implements Read {
	private content: ArrayBuffer;
	private ptr: usize;

	public constructor(content: ArrayBuffer) {
		this.ptr = 0;
		this.content = content;
	}

	read_exact(length: usize): ArrayBuffer | null {
		if (length == 0) { return new ArrayBuffer(0); }
		if (this.ptr + length > this.content.byteLength) {
			return null;
		}
		const res = this.content.slice(this.ptr, this.ptr + length);
		this.ptr += length;
		return res;
	}
}

export class BufWriter implements Write {
	private content: ArrayBuffer[];
	private total_length: usize;

	public constructor() {
		this.content = [];
		this.total_length = 0;
	}

	public res(): ArrayBuffer {
		const buf = new ArrayBuffer(this.total_length);
		const view = new Uint8Array(buf);
		let offset = 0;
		for (const part of this.content) {
			const cur = new Uint8Array(part);
			view.set(cur, offset);
			offset += cur.length;
		}
		return buf;
	}

	write(buf: ArrayBuffer): void {
		this.total_length += buf.byteLength;
		this.content.push(buf);
	}
}
