# Wiring

```text
                         USB
                          |
                          v
                 +-------------------+
                 |      MCP2221      |
                 |                   |
                 |  3V3 ------------+----------------> OLED VCC
                 |  GND ------------+----------------> OLED GND
                 |  SDA ------------+----------------> OLED SDA
                 |  SCL ------------+----------------> OLED SCL
                 |                   |
                 |  GP1 ------------+----[ BUTTON ]--+--> GND
                 +-------------------+       |        |
                                             +--[10K]--+--> 3V3

                 SSD1306 OLED: I2C address 0x3C or 0x3D
                 Button: optional, active-low; polling is enabled by default
```

Notes:

- Connect all grounds together.
- Use the 10K resistor as a pull-up from the GP1/button node to 3.3 V.
- The button connects GP1 to GND when pressed, making the input active-low.
- Button polling is enabled by default; set `MCP2221_ENABLE_BUTTON=0` to disable it.
- The display is typically powered from 3.3 V. Confirm the voltage requirements of your OLED module.
- Use the MCP2221 breakout's pin labels for `SDA`, `SCL`, `3V3`, `GND`, and `GP1`; physical pin numbers vary by breakout board.
