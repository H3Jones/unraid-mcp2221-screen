# Unraid MCP2221 Screen

Tiny Rust service that renders live Unraid metrics on an SSD1306 OLED over an MCP2221 USB-to-I2C bridge.

Built for low resource usage and easy Docker deployment on Unraid.

## What It Shows

- Memory page: RAM used and total
- Storage pages: Array and Cache usage, size, disk count, active disk count
- Page index indicator (for example 1/3)
- Optional button on MCP2221 GP1 to cycle pages

## Hardware

- MCP2221 breakout (example: Adafruit 4471)
- SSD1306 I2C OLED display (commonly 128x64)
- Optional button wired to MCP2221 GP1 (active-low)

Important: this app uses HID access (hidapi), not the MCP2221 UART serial interface.

## Quick Start (Local)

1. Build image:

```bash
docker build -t unraid-mcp2221-screen:local .
```

2. Run dry mode (no hardware required):

```bash
docker run --rm -e MCP2221_DRY_RUN=1 unraid-mcp2221-screen:local
```

3. Run with hardware (simple privileged test):

```bash
docker run --rm \
  --privileged \
  -v /dev/bus/usb:/dev/bus/usb \
  -v /run/udev:/run/udev:ro \
  unraid-mcp2221-screen:local
```

## Unraid Deployment

### Option A: Quick Validation (Privileged)

```bash
docker run --rm \
  --privileged \
  -v /dev/bus/usb:/dev/bus/usb \
  -v /run/udev:/run/udev:ro \
  ghcr.io/h3jones/unraid-mcp2221-screen:master
```

### Option B: Recommended (Non-Privileged With udev)

1. Create persistent udev rule on Unraid host:

```bash
mkdir -p /boot/config/udev/rules.d
cat >/boot/config/udev/rules.d/99-mcp2221.rules <<'EOF'
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="04d8", ATTRS{idProduct}=="00dd", MODE:="0660", GROUP:="users", SYMLINK+="mcp2221"
EOF
```

2. Verify symlink:

```bash
ls -l /dev/mcp2221
```

3. In Unraid Docker template:

- Privileged: No
- Device mapping: /dev/mcp2221:/dev/hidraw0
- Path mapping: /run/udev -> /run/udev (Read Only)

4. Start container.

Equivalent CLI:

```bash
docker run --rm \
  -v /run/udev:/run/udev:ro \
  --device=/dev/mcp2221:/dev/hidraw0 \
  ghcr.io/h3jones/unraid-mcp2221-screen:master
```

## Configuration

Environment variables:

- UNRAID_API_KEY: required, Unraid GraphQL API key
- UNRAID_GRAPHQL_URL: optional endpoint override
- MCP2221_DRY_RUN: set to 1 for console preview without hardware
- MCP2221_ENABLE_BUTTON: set to 1 to enable GP1 button polling

## GraphQL Endpoint Selection

At startup, the app resolves and selects one endpoint.

Priority order:

1. UNRAID_GRAPHQL_URL (if set)
2. http://$HOST_HOSTNAME/graphql
3. http://$HOST_HOSTNAME.local/graphql
4. http://tower.local/graphql

The first resolvable hostname is selected and logged:

```text
[unraid] selected graphql endpoint: ...
```

After startup, the selected endpoint is reused for normal refreshes.

## Runtime Notes

- SSD1306 addresses attempted: 0x3C then 0x3D
- I2C speeds attempted: 100k then 50k
- Button polling shares the same MCP2221 session as display writes to reduce contention

## Troubleshooting

No HID device found:

- Confirm MCP2221 is attached
- Confirm container has correct device access
- Check /run/udev is mounted read-only
- Ensure no other process is exclusively using the MCP2221

Hostname resolution issues:

- Check HOST_HOSTNAME in container environment
- Test DNS from container shell with getent hosts <hostname>
- Set UNRAID_GRAPHQL_URL explicitly as a fallback

GraphQL auth errors:

- Verify UNRAID_API_KEY is set correctly
- Watch for accidental hidden characters in variable names/values

## Development

```bash
cargo check
cargo run
```

## Project State

Current focus is stability and deployment ergonomics on Unraid. Contributions and issue reports are welcome.
