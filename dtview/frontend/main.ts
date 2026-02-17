import { el } from "./dom/elements";
import { WebSocketConnection } from "./utils/ws";

type Ptr = {
	file: string,
	line: number,
};

type Span = {
	ptr: Ptr,
};

type UpdateData = {
	compiled_tree: NodeData,
	sources: { [path: string]: string },
};

type NodeData = {
	defs: Span[],
	properties: { [name: string]: PropertyData },
	nodes: { [name: string]: NodeData },
};

type PropertyData = {
	def: Span,
	value: string[] | null,
};

class ViewMgr {
	private tree: Tree;
	private sources: { [path: string]: Source };
	private root_el: HTMLDivElement;

	public constructor(data: UpdateData) {
		this.tree = new Tree(data.compiled_tree, this);
		this.sources = {};
		for (const entry of Object.entries(data.sources)) {
			this.sources[entry[0]] = new Source(entry[1]);
		}
		this.root_el = el.div([], { classes: ["view"] });
		this.show_tree();
	}

	public show_tree() {
		this.root_el.innerHTML = "";
		this.root_el.appendChild(this.tree.el());
	}

	public show_source(span: Span) {
		const source = this.sources[span.ptr.file]!;
		source.highlight_line(span.ptr.line);
		this.root_el.innerHTML = "";
		this.root_el.appendChild(source.el());
	}

	public el(): HTMLDivElement {
		return this.root_el;
	}
}

class Source {
	private lines: HTMLParagraphElement[];
	private root_el: HTMLDivElement;
	private highlighted: HTMLParagraphElement | null;

	public constructor(content: string) {
		this.highlighted = null;
		this.lines = [];
		for (const line of content.split("\n")) {
			this.lines.push(el.p(line));
		}
		this.root_el = el.div(this.lines, { classes: ["file-content"] });
	}

	public highlight_line(ptr: number) {
		if (this.highlighted !== null) {
			this.highlighted.classList.remove("highlighted");
		}
		const line = this.lines[ptr - 1]!;
		line.scrollIntoView();
		line.classList.add("highlighted")
		this.highlighted = line;
	}

	public el(): HTMLDivElement {
		return this.root_el;
	}
}

class Tree {
	private root_el: HTMLDivElement;

	public constructor(data: NodeData, mgr: ViewMgr) {
		this.root_el = el.div(new Node("/", data, mgr).el(), { classes: ["tree"] });
	}

	public el(): HTMLDivElement {
		return this.root_el;
	}
}

class Node {
	private root_el: HTMLDivElement;

	public constructor(name: string, data: NodeData, mgr: ViewMgr) {
		const properties = [];
		for (const property of Object.entries(data.properties)) {
			properties.push(new Property(property[0], property[1], mgr).el());
		}
		const subnodes = [];
		for (const subnode of Object.entries(data.nodes)) {
			subnodes.push(new Node(subnode[0], subnode[1], mgr).el());
		}
		this.root_el = el.div([
			el.div([
				el.p(name, { classes: ["node-title"] }),
				el.div(properties, { classes: ["node-properties"] }),
			], { classes: ["node-content"] }),
			el.div(subnodes),
		], { classes: ["node-wrapper"] });
	}

	public el(): HTMLDivElement {
		return this.root_el;
	}
}

class Property {
	private root_el: HTMLDivElement;

	public constructor(name: string, data: PropertyData, mgr: ViewMgr) {
		if (data.value === null) {
			this.root_el = el.div([
				el.p(name, { classes: ["property-name"] }),
			], { classes: ["property"] });
		} else {
			this.root_el = el.div([
				el.p(name, { classes: ["property-name"] }),
				el.p(" = "),
				el.p(data.value.join(", "), { classes: ["property-value"] }),
			], { classes: ["property"] });
		}
		this.root_el.addEventListener("click", () => { mgr.show_source(data.def); });
	}

	public el(): HTMLDivElement {
		return this.root_el;
	}
}

function show_error() {
	document.body.innerHTML = "";
	document.body.appendChild(el.p("View update failed, check console", { classes: ["error"] }));
}

async function main() {
	let ws = await WebSocketConnection.connect("/api");
	while (ws.is_ok()) {
		const update = <string | null> await ws.recv();
		if (update === null) {
			show_error();
			return;
		}
		const data: UpdateData | null = JSON.parse(update);
		if (data === null) {
			show_error();
			continue;
		}
		document.body.innerHTML = "";
		document.body.appendChild(new ViewMgr(data).el());
	}
}

main();
