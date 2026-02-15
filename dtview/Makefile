../target/debug/dtview: $(shell find src -type f) Cargo.toml ../Cargo.lock \
	target/script.js target/index.html target/style.css
	cargo build

.PHONY: run
run: ../target/debug/dtview
	$< example.dts

node_modules:
	bun install

target:
	mkdir $@

target/%.html: frontend/%.html node_modules | target
	cat $< | bun run html-minifier --collapse-whitespace --remove-attribute-quotes \
		--remove-comments --remove-tag-whitespace --sort-attributes --sort-class-name \
		--use-short-doctype -o $@

target/script.js: frontend/main.ts $(shell find frontend -type f -regex '^.*\.ts$$') node_modules \
	| target
	bun build --minify $< > $@

target/style.css: frontend/style.scss $(shell find frontend -type f -regex '^.*\.scss$$') \
	node_modules | target
	bun run sass --style compressed $< > $@
