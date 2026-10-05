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

# Local hardware test with GP1 button page-cycling enabled.
run-hw-button:
	$env:MCP2221_ENABLE_BUTTON='1'; cargo run

# Hardware-free validation; exercises compile-time checks and automated tests only.
verify-local:
	cargo check; cargo test
