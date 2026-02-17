import { format_debug, format_display, type Debug, type Display, type Write } from "./fmt";

export abstract class Drop {
	public abstract drop(): void;

	public async with_drop<R>(run: (value: this) => Promise<R> | R): Promise<R> {
		let res;
		try {
			res = run(this);
		} catch {
			throw new Error("errored during a with_drop call");
		}
		if (res instanceof Promise) {
			return res.finally(() => { this.drop(); });
		} else {
			this.drop();
			return res;
		}
	}
}

export class Ref<T> implements Debug, Display {
	private value: T;

	public constructor(value: T) {
		this.value = value;
	}

	public get v(): T {
		return this.value;
	}

	public set v(value: T) {
		this.v = value;
	}

	fmt_dbg(writer: Write<string>): void {
	    writer.write("Ref(");
		format_debug(this.value, writer);
		writer.write(")");
	}

	fmt_display(writer: Write<string>): void {
	    format_display(this.value, writer);
	}
}

export class Vec<T> implements Debug, Display {
	private values: T[];

	public constructor() {
		this.values = [];
	}

	public static from<T>(values: T[]): Vec<T> {
		const res = new Vec<T>();
		res.values = values;
		return res;
	}

	public static with_capacity<T>(capacity: usize): Vec<T> {
		const res = new Vec<T>();
		res.values = new Array(capacity);
		return res;
	}

	public push(value: T) {
		this.values.push(value);
	}

	public shift(): T | null {
		const v = this.values.shift();
		if (v === undefined) {
			return null;
		}
		return v;
	}

	public req_shift(): T {
		const v = this.values.shift();
		if (v === undefined) {
			throw new Error("tried to shift an empty vector");
		}
		return v;
	}

	public get(index: usize): T | null {
		const v = this.values.at(index);
		if (v === undefined) {
			return null;
		}
		return v;
	}

	public req(index: usize): T {
		const v = this.values[index];
		if (v === undefined) {
			throw new Error("required an out of bounds element from a vector");
		}
		return v;
	}

	public len(): usize {
		return this.values.length;
	}

	public deref(): T[] {
		return this.values;
	}

	fmt_dbg(writer: Write<string>): void {
	    writer.write("[");
		let first = true;
		for (const v of this.values) {
			if (first) {
				first = false;
			} else {
				writer.write(", ");
			}
			format_debug(v, writer);
		}
		writer.write("]");
	}

	fmt_display(writer: Write<string>): void {
		let first = true;
		for (const v of this.values) {
			if (first) {
				first = false;
			} else {
				writer.write(", ");
			}
			format_display(v, writer);
		}
	}
}

export type bool = boolean;
/**
* Type hint. Safely storable in JavaScript's `number` type.
*/
export type i32 = number;
/**
* Type hint. Safely storable in JavaScript's `number` type.
*/
export type u32 = number;
/**
* Type hint. Safely storable in JavaScript's `number` type.
*/
export type usize = u32;
/**
* Type hint. Default JavaScript `number` type.
*/
export type f64 = number;

export class Option<T> implements Debug, Display {
	private value: {
		is_some: false
	} | {
		is_some: true,
		value: T,
	};

	public constructor() {
		this.value = { is_some: false };
	}

	public static none<T>(): Option<T> {
		return new Option();
	}

	public static some<T>(value: T): Option<T> {
		const res = new Option<T>();
		res.value = { is_some: true, value };
		return res;
	}

	public set_none() {
		this.value = { is_some: false };
	}

	public set_some(value: T) {
		this.value = { is_some: true, value };
	}

	public deref(): T {
		if (!this.value.is_some) {
			throw new Error("tried to deref a value from a None option");
		}
		return this.value.value;
	}

	public is_some(): bool {
		return this.value.is_some;
	}

	public map<R>(mapper: (v: T) => R): Option<R> {
		if (this.is_some()) {
			return Option.some(mapper(this.deref()));
		} else {
			return Option.none();
		}
	}

	fmt_dbg(writer: Write<string>): void {
		if (this.value.is_some) {
			writer.write("Some(");
			format_debug(this.value.value, writer);
			writer.write(")");
		} else {
			writer.write("None");
		}
	}

	fmt_display(writer: Write<string>): void {
	    if (this.value.is_some) {
			format_display(this.value.value, writer);
		} else {
			writer.write("None");
		}
	}
}
