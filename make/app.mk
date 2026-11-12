.PHONY: dev build deb
dev:
	@pids=$$(lsof -t -nP -iTCP:27502 -sTCP:LISTEN); \
	if [ -n "$$pids" ]; then \
		owned_pids=$$(lsof -t -nP -a -u "$$(id -un)" -iTCP:27502 -sTCP:LISTEN); \
		if [ "$$pids" != "$$owned_pids" ]; then \
			echo "Port 27502 is used by a process owned by another user:"; \
			lsof -nP -iTCP:27502 -sTCP:LISTEN; \
			exit 1; \
		fi; \
		echo "Stopping process on port 27502 (PID: $$pids)"; \
		kill $$pids || exit 1; \
		for attempt in 1 2 3 4 5 6 7 8 9 10; do \
			if ! lsof -nP -iTCP:27502 -sTCP:LISTEN >/dev/null 2>&1; then break; fi; \
			sleep 0.2; \
		done; \
		if lsof -nP -iTCP:27502 -sTCP:LISTEN >/dev/null 2>&1; then \
			echo "Port 27502 is still in use after stopping the process"; \
			exit 1; \
		fi; \
	fi
	@$(PNPM) tauri:dev

build:
	@$(PNPM) build

# 打 Linux deb 并修补依赖声明（Tauri 的 depends 是追加，无法覆盖自动探测结果）
deb:
	@rm -rf src-tauri/target/release/bundle/deb
	@$(PNPM) tauri build --bundles deb
	@node scripts/patch-deb-depends.mjs
