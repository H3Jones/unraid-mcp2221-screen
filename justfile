set shell := ["pwsh.exe", "-NoLogo", "-Command"]

_default:
	@just --list

check:
	cargo check

test:
	cargo test

# Local test without hardware attached.
run-dry:
	$env:MCP2221_DRY_RUN='1'; cargo run

# Local test with MCP2221/OLED hardware attached.
run-hw:
	cargo run

# Local hardware test with GP1 button page-cycling enabled.
run-hw-button:
	$env:MCP2221_ENABLE_BUTTON='1'; cargo run

# Full local validation pass.
verify-local:
	cargo check; cargo test; $env:MCP2221_DRY_RUN='1'; cargo run
