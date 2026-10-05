set shell := ["pwsh.exe", "-NoLogo", "-Command"]

_default:
	@just --list

check:
	cargo check

test:
	cargo test

# Run the application with MCP2221/OLED hardware attached.
run-hw:
	cargo run

# Hardware-free validation; exercises compile-time checks and automated tests only.
verify-local:
	cargo check; cargo test
