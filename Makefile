.PHONY: build release test lint fmt check clean install release-patch release-minor release-major

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

lint:
	cargo fmt -- --check
	cargo clippy -- -D warnings

fmt:
	cargo fmt

check: lint test

clean:
	cargo clean

install: release
	cp target/release/unifi-cli ~/.local/bin/unifi-cli

release-patch:
	vership bump patch

release-minor:
	vership bump minor

release-major:
	vership bump major
