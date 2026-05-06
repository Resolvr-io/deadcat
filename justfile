install:
	pnpm install

dev:
	cargo tauri dev

dev-manager:
	cd src-tauri-manager && cargo tauri dev

biome-lint:
	biome lint .

biome-format:
	biome format .

biome-check:
	biome check --error-on-warnings .

biome-fix:
	biome check --write --unsafe .

tsc:
	pnpm tsc

unit-test:
	pnpm test:unit

screenshots-install:
	@if [ -n "${PUPPETEER_EXECUTABLE_PATH-}" ] && [ -x "${PUPPETEER_EXECUTABLE_PATH-}" ]; then :; else pnpm exec puppeteer browsers install chrome; fi

screenshots: screenshots-install
	pnpm test:screenshots

screenshots-update: screenshots-install
	pnpm test:screenshots:update

cargo-fmt:
	cd src-tauri && cargo fmt --all -- --check

cargo-clippy:
	cd src-tauri && cargo clippy --all-targets -- -D warnings

cargo-test:
	cd src-tauri && ulimit -n 10240 && \
		ARCH="$(uname -m)-$(uname -s | tr '[:upper:]' '[:lower:]')"; \
		case "$ARCH" in \
			arm64-darwin)  TRIPLE="aarch64-apple-darwin" ;; \
			x86_64-linux)  TRIPLE="x86_64-unknown-linux-gnu" ;; \
			*) echo "Unsupported platform: $ARCH" >&2; exit 1 ;; \
		esac; \
		export ELEMENTSD_EXEC=$PWD/crates/deadcat-sdk/tests/elementsd-$TRIPLE; \
		export ELECTRS_LIQUID_EXEC=$PWD/crates/deadcat-sdk/tests/electrs-$TRIPLE; \
		cargo nextest run --workspace && \
		cargo test --doc --workspace
