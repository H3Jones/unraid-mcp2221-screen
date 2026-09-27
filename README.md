# Unraid MCP2221 Powered Screen

Minimal Rust container for proving MCP2221 USB access from Docker by writing `Hello` to an SSD1306 128x64 display over I2C.

## Local First Test Plan

1. Build locally:

   ```bash
   docker build -t unraid-mcp2221-screen:local .
   ```
2. Verify container startup without hardware:

   ```bash
   docker run --rm -e MCP2221_DRY_RUN=1 unraid-mcp2221-screen:local
   ```
3. Linux/Unraid hardware run (USB pass-through required):

   ```bash
   docker run --rm --privileged -v /dev/bus/usb:/dev/bus/usb unraid-mcp2221-screen:local
   ```

## Unraid Pass-Through Notes

If you can see this on the Unraid host:

```bash
ls /dev/serial/by-id
usb-Microchip_Technology_Inc._MCP2221_USB-I2C_UART_Combo-if00
```

the device is attached, but this Rust app uses HID access (via hidapi), not the UART serial interface.

For MCP2221 in Docker, the most reliable run is:

```bash
docker run --rm \
   --privileged \
   -v /dev/bus/usb:/dev/bus/usb \
   -v /run/udev:/run/udev:ro \
   unraid-mcp2221-screen:local
```

Quick container-side visibility check:

```bash
docker run --rm --privileged -v /dev/bus/usb:/dev/bus/usb -v /run/udev:/run/udev:ro debian:bookworm-slim sh -lc 'ls /dev/hidraw* 2>/dev/null || true; ls /dev/bus/usb/*/* | head'
```

If the app still reports "No HID devices with requested VID/PID found", verify the container is running privileged and the MCP2221 is not claimed by another process.

## No-Privileged Setup With udev (Recommended)

You can run without privileged mode by mapping one stable device node.

1. Create a persistent udev rule on the Unraid host:

   ```bash
   mkdir -p /boot/config/udev/rules.d
   cat >/boot/config/udev/rules.d/99-mcp2221.rules <<'EOF'
   SUBSYSTEM=="hidraw", ATTRS{idVendor}=="04d8", ATTRS{idProduct}=="00dd", MODE:="0660", GROUP:="users", SYMLINK+="mcp2221"
   EOF
   ```
2. Verify the stable symlink exists:

   ```bash
   ls -l /dev/mcp2221
   ```
3. In the Unraid Docker template, use:

   - Privileged: `No`
   - Config Type: `Device`
     - Name: `MCP2221 HID`
     - Value: `/dev/mcp2221:/dev/hidraw0`
   - Config Type: `Path`
     - Host Path: `/run/udev`
     - Container Path: `/run/udev`
     - Access Mode: `Read Only`
4. mkdir -p /etc/udev/rules.d
5. Start the container and test.

Why `/dev/hidraw0` in the container?

- `hidapi` opens the hidraw path it enumerates (for example `/dev/hidraw0`).
- Mapping `/dev/mcp2221` to `/dev/hidraw0` keeps host-side stability while matching the in-container path that `hidapi` opens.

Equivalent CLI run:

```bash
docker run --rm \
   -v /run/udev:/run/udev:ro \
   --device=/dev/mcp2221:/dev/hidraw0 \
   ghcr.io/h3jones/unraid-mcp2221-screen:master
```

## Notes

- The binary tries SSD1306 addresses `0x3C` then `0x3D`.
- It tries I2C bus speeds `100k` and `50k`.
- `MCP2221_DRY_RUN=1` skips all hardware access and is intended for local sanity checks.
- One page-cycle button is wired on MCP2221 `GP1` (active-low). A press advances to the next screen.
- Button polling is disabled by default; set `MCP2221_ENABLE_BUTTON=1` to enable it.
- Button polling now uses the same MCP2221 session as display rendering to avoid dual-connection bus contention.

## Project Status

- [X] Test deployment to private ghcr and pull from unraid
- [X] Deploy to unraid and test mcp2221 with example output
- [ ] Integrate with unriad api to pull metrics
