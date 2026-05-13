.PHONY: fmt check test node cli chat browser browser-desktop gofmt

fmt:
	cargo fmt --all
	gofmt -w sdk/go

check:
	cargo check --workspace --all-targets
	go test ./sdk/go/...

test:
	cargo test --workspace
	go test ./sdk/go/...

node:
	cargo run -p void-node

cli:
	cargo run -p void-cli -- --help

peers:
	cargo run -p void-cli -- peers

topology:
	cargo run -p void-cli -- topology

chat:
	cargo run -p void-chat

browser:
	cargo run -p void-browser

browser-desktop:
	bash scripts/run-void-browser.sh

gofmt:
	gofmt -w sdk/go
