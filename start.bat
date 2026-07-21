@echo off
cd /d "%~dp0"
set RUST_LOG=debug
bun run tauri dev
