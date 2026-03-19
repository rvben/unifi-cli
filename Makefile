.PHONY: build release test lint fmt check clean install version-get version-patch version-minor version-push release-patch release-minor

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

# Version management
version-get:
	@echo "Current version: $$(git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0)"
	@echo "Cargo.toml version: $$(grep '^version' Cargo.toml | head -1 | sed -E 's/version = "(.*)"/\1/')"

version-patch:
	@echo "Creating new patch version..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval MINOR := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.([0-9]+)\.[0-9]+/\1/'))
	$(eval PATCH := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.[0-9]+\.([0-9]+)/\1/'))
	$(eval NEW_PATCH := $(shell echo $$(( $(PATCH) + 1 ))))
	$(eval NEW_TAG := v$(MAJOR).$(MINOR).$(NEW_PATCH))
	$(eval VERSION_NO_V := $(MAJOR).$(MINOR).$(NEW_PATCH))
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@sed -i.bak -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+"/version = "$(VERSION_NO_V)"/' Cargo.toml && rm -f Cargo.toml.bak
	@cargo check --quiet
	@git add Cargo.toml Cargo.lock
	@git commit -m "chore: bump version to $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Version $(NEW_TAG) created. Run 'make version-push' to trigger release."

version-minor:
	@echo "Creating new minor version..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval MINOR := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.([0-9]+)\.[0-9]+/\1/'))
	$(eval NEW_MINOR := $(shell echo $$(( $(MINOR) + 1 ))))
	$(eval NEW_TAG := v$(MAJOR).$(NEW_MINOR).0)
	$(eval VERSION_NO_V := $(MAJOR).$(NEW_MINOR).0)
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@sed -i.bak -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+"/version = "$(VERSION_NO_V)"/' Cargo.toml && rm -f Cargo.toml.bak
	@cargo check --quiet
	@git add Cargo.toml Cargo.lock
	@git commit -m "chore: bump version to $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Version $(NEW_TAG) created. Run 'make version-push' to trigger release."

version-push:
	$(eval LATEST_TAG := $(shell git describe --tags --abbrev=0))
	@echo "Pushing commit and tag $(LATEST_TAG)..."
	@git push origin main $(LATEST_TAG)
	@echo "Release workflow triggered for $(LATEST_TAG)"

release-patch: version-patch version-push
release-minor: version-minor version-push
