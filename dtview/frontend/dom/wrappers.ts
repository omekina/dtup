import { format_debug, format_display, type Debug, type Display, type Write } from "../std/fmt";
import { Option } from "../std/mem";

export interface ElementWrapper<T extends HTMLElement> {
	el(): T;
}

export class Element<T extends HTMLElement> implements ElementWrapper<T>, Debug, Display {
	private dom: T;

	public constructor(inner: T) {
		this.dom = inner;
	}

	fmt_dbg(writer: Write<string>): void {
		if (this.dom.tagName in ["p", "h1", "h2", "h3", "h4", "h5", "h6"]) {
			writer.write("DomElement { type: ");
			format_debug(this.dom.tagName, writer);
			writer.write(", text: ");
			format_debug(this.dom.innerText, writer);
			writer.write(" }");
		} else {
			writer.write("DomElement(");
			format_debug(this.dom.tagName, writer);
			writer.write(")");
		}
	}

	fmt_display(writer: Write<string>): void {
	    writer.write("DomElement");
	}

	el(): T {
	    return this.dom;
	}
}

export class Draggable<
	E extends HTMLElement, T extends ElementWrapper<E>
> implements ElementWrapper<E>, Debug, Display {
	private inner: T;
	private drag: Option<{ prev_x: number, prev_y: number }>;
	private callback: (diff_x: number, dif_y: number) => void;

	public constructor(inner: T, callback: (diff_x: number, diff_y: number) => void) {
		this.inner = inner;
		this.drag = Option.none();
		this.callback = callback;
		const el = this.inner.el();
		el.addEventListener("mousedown", (e) => {
			if (e.defaultPrevented) { return; }
			this.drag_start(e.clientX, e.clientY);
			e.preventDefault();
		})
		window.addEventListener("mousemove", (e) => {
			this.drag_move(e.clientX, e.clientY);
		});
		window.addEventListener("mouseup", () => {
			this.drag_end();
		});
	}

	private drag_start(x: number, y: number) {
		if (this.drag.is_some()) { return; }
		this.drag.set_some({
			prev_x: x,
			prev_y: y,
		});
	}

	private drag_end() {
		this.drag.set_none();
	}

	private drag_move(x: number, y: number) {
		if (!this.drag.is_some()) { return; }
		const status = this.drag.deref();
		const dx = x - status.prev_x;
		const dy = y - status.prev_y;
		status.prev_x = x;
		status.prev_y = y;
		this.callback(dx, dy);
	}

	fmt_dbg(writer: Write<string>): void {
	    writer.write("Draggable(");
		format_debug(this.inner, writer);
		writer.write(")");
	}

	fmt_display(writer: Write<string>): void {
	    format_display(this.inner, writer);
	}

	el(): E {
	    return this.inner.el();
	}
}

export class Clickable<
	E extends HTMLElement, T extends ElementWrapper<E>
> implements ElementWrapper<E>, Debug, Display {
	private inner: T;

	public constructor(inner: T, callback: () => void) {
		this.inner = inner;
		this.inner.el().addEventListener("click", () => {
			callback();
		});
	}

	fmt_dbg(writer: Write<string>): void {
	    writer.write("Clickable(");
		format_debug(this.inner, writer);
		writer.write(")");
	}

	fmt_display(writer: Write<string>): void {
		format_display(this.inner, writer);
	}

	el(): E {
		return this.inner.el();
	}
}
