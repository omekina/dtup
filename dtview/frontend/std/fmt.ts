import { Vec } from "./mem";

export interface Write<T> {
	write(value: T): void;
}

export interface Debug {
	fmt_dbg(writer: Write<string>): void;
}

export interface Display {
	fmt_display(writer: Write<string>): void;
}

export class BufWriter<T> implements Write<T> {
	private buf: Vec<T>;

	public constructor() {
		this.buf = new Vec();
	}

	public get result(): Vec<T> {
		return this.buf;
	}

	write(value: T): void {
	    this.buf.push(value);
	}
}

function fallback_display<T>(value: T): string {
	if (value === undefined || value === null) {
		return "None";
	} else if (typeof value == "object") {
		return "Object";
	} else if (typeof value == "string") {
		return value;
	} else {
		return String(value);
	}
}

function format_string(value: string): string {
	return JSON.stringify(value);
}

export function format_debug<T, W extends Write<string>>(value: T, writer: W) {
	if (typeof value === "object" && value !== null && "fmt_dbg" in value) {
		(<Debug>value).fmt_dbg(writer);
	} else if (typeof value == "string") {
		writer.write(format_string(value));
	} else {
		writer.write(fallback_display(value));
	}
}

export function format_display<T, W extends Write<string>>(value: T, writer: W) {
	if (typeof value === "object" && value !== null && "fmt_display" in value) {
		(<Display>value).fmt_display(writer);
	} else if (typeof value == "string") {
		writer.write(value);
	} else {
		format_debug(value, writer);
	}
}

export function debug<T>(value: T): string {
	if (typeof value === "object" && value !== null && "fmt_dbg" in value) {
		const writer = new BufWriter();
		(<Debug>value).fmt_dbg(writer);
		return writer.result.deref().join("");
	} else if (typeof value === "string") {
		return format_string(value);
	} else {
		return fallback_display(value);
	}
}

export function display<T>(value: T): string {
	if (typeof value === "object" && value !== null && "fmt_display" in value) {
		const writer = new BufWriter();
		(<Display>value).fmt_display(writer);
		return writer.result.deref().join("");
	} else if (typeof value == "string") {
		return value;
	} else {
		return debug(value);
	}
}
