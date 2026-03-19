#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'
ERRORS=0

pass() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; ERRORS=$((ERRORS + 1)); }

echo "=== Verifying release readiness ==="
echo

# 1. No uncommitted changes
if git diff --quiet && git diff --cached --quiet; then
    pass "No uncommitted changes"
else
    fail "Uncommitted changes detected — commit or stash first"
fi

# 2. Cargo.lock is up to date
if cargo check --locked --quiet 2>/dev/null; then
    pass "Cargo.lock is up to date"
else
    fail "Cargo.lock is out of date — run 'cargo check' and commit Cargo.lock"
fi

# 3. Extract version from Cargo.toml
CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed -E 's/version = "(.*)"/\1/')
echo
echo "  Cargo.toml version: ${CARGO_VERSION}"

# 4. Check that the tag doesn't already exist
if git rev-parse "v${CARGO_VERSION}" >/dev/null 2>&1; then
    fail "Tag v${CARGO_VERSION} already exists — bump version in Cargo.toml"
else
    pass "Tag v${CARGO_VERSION} does not exist yet"
fi

# 5. CHANGELOG has an entry for this version
if [ -f CHANGELOG.md ]; then
    if grep -q "\[${CARGO_VERSION}\]" CHANGELOG.md; then
        pass "CHANGELOG.md has entry for ${CARGO_VERSION}"
    else
        fail "CHANGELOG.md missing entry for ${CARGO_VERSION}"
    fi
else
    fail "CHANGELOG.md not found"
fi

# 6. Lint passes
echo
echo "  Running lint..."
if make lint >/dev/null 2>&1; then
    pass "Lint passes"
else
    fail "Lint failed — run 'make lint' to see errors"
fi

# 7. Tests pass
echo "  Running tests..."
if make test >/dev/null 2>&1; then
    pass "Tests pass"
else
    fail "Tests failed — run 'make test' to see errors"
fi

# Summary
echo
if [ "$ERRORS" -eq 0 ]; then
    echo -e "${GREEN}All checks passed!${NC} Ready to release v${CARGO_VERSION}"
    echo
    echo "Next steps:"
    echo "  git tag -a v${CARGO_VERSION} -m \"Release v${CARGO_VERSION}\""
    echo "  git push origin main v${CARGO_VERSION}"
else
    echo -e "${RED}${ERRORS} check(s) failed.${NC} Fix the issues above before releasing."
    exit 1
fi
