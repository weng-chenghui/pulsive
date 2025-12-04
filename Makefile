# Pulsive Makefile
# Mirrors CI checks for local development

# Use same settings as CI
export CARGO_TERM_COLOR := always
export RUSTFLAGS := -Dwarnings
export RUSTDOCFLAGS := -Dwarnings

# Exclude pulsive-godot (requires special setup) by default
WORKSPACE_EXCLUDE := --exclude pulsive-godot

.PHONY: all check fmt clippy build test docs clean help pre-push godot

# Default target - run all checks (same as pre-push)
all: check

# Run all CI checks (excluding integration tests)
check: fmt clippy build test docs
	@echo "✅ All checks passed!"

# Check formatting
fmt:
	@echo "📝 Checking formatting..."
	cargo fmt --all -- --check

# Format code (fix formatting issues)
fmt-fix:
	@echo "📝 Fixing formatting..."
	cargo fmt --all

# Run clippy linter
clippy:
	@echo "🔍 Running clippy..."
	cargo clippy --workspace $(WORKSPACE_EXCLUDE) -- -D warnings

# Build the workspace
build:
	@echo "🔨 Building..."
	cargo build --workspace $(WORKSPACE_EXCLUDE)

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test --workspace $(WORKSPACE_EXCLUDE)

# Build documentation
docs:
	@echo "📚 Building documentation..."
	cargo doc --workspace $(WORKSPACE_EXCLUDE) --no-deps

# Build pulsive-godot (optional, requires Godot setup)
godot:
	@echo "🎮 Building pulsive-godot..."
	cargo build -p pulsive-godot

# Run integration tests (requires Docker)
integration:
	@echo "🐳 Running integration tests..."
	cd examples/http_server/integration-test && docker compose up --build --abort-on-container-exit

# Clean build artifacts
clean:
	@echo "🧹 Cleaning..."
	cargo clean

# Alias for check (used by git hook)
pre-push: check

# Quick check (just build + test, skip lints)
quick:
	@echo "⚡ Quick build + test..."
	cargo build --workspace $(WORKSPACE_EXCLUDE)
	cargo test --workspace $(WORKSPACE_EXCLUDE)

# Install git hooks
install-hooks:
	@echo "🔗 Installing git hooks..."
	@mkdir -p .git/hooks
	@cp scripts/pre-push .git/hooks/pre-push
	@chmod +x .git/hooks/pre-push
	@echo "✅ Git hooks installed!"

# Show help
help:
	@echo "Pulsive Makefile"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all          Run all CI checks (default)"
	@echo "  check        Run all CI checks (fmt, clippy, build, test, docs)"
	@echo "  fmt          Check code formatting"
	@echo "  fmt-fix      Fix code formatting"
	@echo "  clippy       Run clippy linter"
	@echo "  build        Build the workspace"
	@echo "  test         Run tests"
	@echo "  docs         Build documentation"
	@echo "  godot        Build pulsive-godot (optional)"
	@echo "  integration  Run Docker integration tests"
	@echo "  clean        Clean build artifacts"
	@echo "  quick        Quick build + test (skip lints)"
	@echo "  install-hooks Install git pre-push hook"
	@echo "  help         Show this help"

