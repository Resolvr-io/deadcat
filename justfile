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

screenshots-install:
	@if [ -n "${PUPPETEER_EXECUTABLE_PATH-}" ] && [ -x "${PUPPETEER_EXECUTABLE_PATH-}" ]; then :; else pnpm exec puppeteer browsers install chrome; fi

screenshots: screenshots-install
	pnpm test:screenshots

screenshots-update: screenshots-install
	pnpm test:screenshots:update

test-sdk:
	cd src-tauri/crates/deadcat-sdk && ulimit -n 10240 && ELEMENTSD_EXEC=$PWD/tests/elementsd ELECTRS_LIQUID_EXEC=$PWD/tests/electrs cargo test --test sdk -- --test-threads=1
