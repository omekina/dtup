import { format_debug, format_display, type Debug, type Display, type Write } from "./fmt";
import { type bool, type usize, Option } from "./mem";

export class HashMap<K, V> implements Debug, Display {
	private inner: Map<K, V>;

	public constructor() {
		this.inner = new Map();
	}

	public static from<K, V>(values: [K, V][]): HashMap<K, V> {
		const res = new HashMap<K, V>();
		for (const [k, v] of values) {
			res.insert(k, v);
		}
		return res;
	}

	public insert(key: K, value: V): Option<V> {
		const ret = this.get(key);
		this.inner.set(key, value);
		return ret;
	}

	public get(key: K): Option<V> {
		const v = this.inner.get(key);
		if (v === undefined) {
			return Option.none();
		}
		return Option.some(v);
	}

	public contains_key(key: K): bool {
		return this.inner.has(key);
	}

	public keys(): IteratorObject<K> {
		return this.inner.keys();
	}

	public values(): IteratorObject<V> {
		return this.inner.values();
	}

	public len(): usize {
		return this.inner.size;
	}

	fmt_dbg(writer: Write<string>): void {
	    writer.write("HashMap { ");
		let first = true;
		for (const [k, v] of this.inner) {
			if (first) {
				first = false;
			} else {
				writer.write(", ");
			}
			format_debug(k, writer);
			writer.write(": ");
			format_debug(v, writer);
		}
		writer.write(" }");
	}

	fmt_display(writer: Write<string>): void {
		let first = true;
		for (const [k, v] of this.inner) {
			if (first) {
				first = false;
			} else {
				writer.write(", ");
			}
			format_display(k, writer);
			writer.write(": ");
			format_display(v, writer);
		}
	}
}
