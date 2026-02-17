export namespace el {
	type ElementOptions = {
		classes: string[],
	};

	function apply_options(el: HTMLElement, opts: ElementOptions) {
		el.classList.add(...opts.classes);
	}

	function el<T extends HTMLElement>(tag: string, opts?: ElementOptions): T {
		const res = <T>document.createElement(tag);
		if (opts !== undefined) {
			apply_options(res, opts);
		}
		return res;
	}

	type Content = HTMLElement | HTMLElement[];

	function with_content<T extends HTMLElement>(
		tag: string, content?: HTMLElement | HTMLElement[], opts?: ElementOptions
	): T {
		const res: T = el(tag, opts);
		if (content !== undefined) {
			if (Array.isArray(content)) {
				res.append(...content);
			} else {
				res.append(content);
			}
		}
		return res;
	}

	function with_text<T extends HTMLElement>(
		tag: string, text?: string, opts?: ElementOptions,
	): T {
		const res: T = el(tag, opts);
		if (text !== undefined) {
			res.innerText = text;
		}
		return res;
	}

	type VariableContent = string | Content;

	function with_variable<T extends HTMLElement>(
		tag: string, content?: string | Content, opts?: ElementOptions,
	): T {
		if (content !== undefined) {
			if (typeof content === "string") {
				return with_text(tag, content, opts);
			} else if (typeof content === "object") {
				return with_content(tag, content, opts);
			}
		}
		return el(tag, opts);
	}

	export function heading(
		level: number, text: string, opts?: ElementOptions
	): HTMLHeadingElement {
		if (Number.isInteger(level) && level >= 1 && level <= 6) {
			return with_text(`h${level}`, text, opts);
		} else {
			throw new Error("invalid heading level");
		}
	}

	export function h1(text: string, opts?: ElementOptions): HTMLHeadingElement {
		return heading(1, text, opts);
	}

	export function h2(text: string, opts?: ElementOptions): HTMLHeadingElement {
		return heading(2, text, opts);
	}

	export function h3(text: string, opts?: ElementOptions): HTMLHeadingElement {
		return heading(3, text, opts);
	}

	export function p(text: string, opts?: ElementOptions): HTMLParagraphElement {
		return with_text("p", text, opts);
	}

	export function div(content: Content, opts?: ElementOptions): HTMLDivElement {
		return with_content("div", content, opts);
	}

	type SubmitableTextInput = "text" | "password";

	export function submitable_input(
		of_type: SubmitableTextInput,
		change_callback: (value: string) => void,
		submit_callback: (value: string) => void,
		placeholder?: string,
		opts?: ElementOptions
	): HTMLInputElement {
		const res: HTMLInputElement = el("input", opts);
		res.type = of_type;
		res.addEventListener("change", () => {
			change_callback(res.value);
		});
		res.addEventListener("keypress", (e) => {
			if (e.defaultPrevented) {
				return;
			}
			if (e.key === "Enter") {
				submit_callback(res.value);
				e.preventDefault();
			}
		});
		if (placeholder !== undefined) {
			res.placeholder = placeholder;
		}
		return res;
	}

	export function button(
		content: VariableContent, callback: () => void, opts?: ElementOptions
	): HTMLButtonElement {
		const res: HTMLButtonElement = with_variable("button", content, opts);
		res.addEventListener("click", () => {
			callback();
		});
		return res;
	}
}
