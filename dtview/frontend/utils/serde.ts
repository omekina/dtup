import { type Read, type Write } from "../std/io";
import type { bool, f64, u32, usize } from "../std/mem";

export namespace ser {
	export function ser_u32(value: u32, writer: Write) {
		const buf = new ArrayBuffer(4);
		const view = new DataView(buf);
		view.setUint32(0, value, true);
		writer.write(buf);
	}

	export function ser_f32(value: f64, writer: Write) {
		const buf = new ArrayBuffer(4);
		const view = new DataView(buf);
		view.setFloat32(0, value, true);
		writer.write(buf);
	}

	export function ser_bool(value: bool, writer: Write) {
		ser_byte(value ? 1 : 0, writer);
	}

	export function ser_byte(value: u32, writer: Write) {
		const buf = new Uint8Array([value]);
		writer.write(buf.buffer);
	}

	export function ser_raw_string(value: string, writer: Write) {
		const encoder = new TextEncoder();
		writer.write(encoder.encode(value).buffer);
	}
}

export namespace de {
	export function de_u32(reader: Read): u32 | null {
		const read = reader.read_exact(4);
		if (read === null) { return null; }
		const view = new DataView(read);
		return view.getUint32(0, true);
	}

	export function de_f32(reader: Read): f64 | null {
		const read = reader.read_exact(4);
		if (read === null) { return null; }
		const view = new DataView(read);
		return view.getFloat32(0, true);
	}

	export function de_bool(reader: Read): bool | null {
		const read = reader.read_exact(1);
		if (read === null) { return null; }
		const value = new Uint8Array(read)[0];
		if (value === 0) {
			return false;
		} else if (value === 1) {
			return true;
		} else {
			return null;
		}
	}

	export function de_byte(reader: Read): u32 | null {
		const read = reader.read_exact(1);
		if (read === null) { return null; }
		const value = new Uint8Array(read)[0];
		if (value === undefined) { return null; }
		return value;
	}

	export function de_raw_string(length: usize, reader: Read): string | null {
		const read = reader.read_exact(length);
		if (read === null) { return null; }
		const decoder = new TextDecoder();
		return decoder.decode(read);
	}
}
