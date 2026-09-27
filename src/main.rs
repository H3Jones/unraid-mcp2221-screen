use std::env;

use embedded_graphics::{
    mono_font::{MonoTextStyleBuilder, ascii::FONT_10X20},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use mcp2221_hal::{MCP2221, i2c::I2cSpeed};
use ssd1306::{
    I2CDisplayInterface, Ssd1306,
    mode::DisplayConfig,
    prelude::{DisplayRotation, DisplaySize128x64},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env_flag_enabled("MCP2221_DRY_RUN") {
        println!("Dry run enabled. Skipping hardware access.");
        println!("Would try addresses [0x3C, 0x3D] at speeds [100000, 50000].");
        return Ok(());
    }

    let mut last_err: Option<std::io::Error> = None;

    for speed in [I2cSpeed::standard_100k(), I2cSpeed::new(50_000)] {
        for addr in [0x3C_u8, 0x3D_u8] {
            println!("Trying SSD1306 128x64 at 0x{addr:02X}, {} bps", speed.speed());

            match try_ssd1306_hello(addr, speed) {
                Ok(()) => {
                    println!("Rendered Hello on 128x64 at 0x{addr:02X}, {} bps", speed.speed());
                    return Ok(());
                }
                Err(e) => {
                    println!("Failed on 0x{addr:02X} at {} bps: {e}", speed.speed());
                    last_err = Some(e);
                }
            }
        }
    }

    Err(last_err
        .unwrap_or_else(|| std::io::Error::other("SSD1306 128x64 Hello test failed"))
        .into())
}

fn env_flag_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        }
        Err(_) => false,
    }
}

fn try_ssd1306_hello(addr: u8, speed: I2cSpeed) -> Result<(), std::io::Error> {
    let mcp = MCP2221::connect().map_err(|e| std::io::Error::other(format!("connect failed: {e:?}")))?;

    mcp.i2c_set_bus_speed(speed)
        .map_err(|e| std::io::Error::other(format!("set bus speed failed: {e:?}")))?;

    let _ = mcp.i2c_cancel_transfer();

    let interface = I2CDisplayInterface::new_custom_address(mcp, addr);
    let mut display =
        Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0).into_buffered_graphics_mode();

    display
        .init()
        .map_err(|e| std::io::Error::other(format!("display init failed: {e:?}")))?;

    display
        .clear(BinaryColor::Off)
        .map_err(|e| std::io::Error::other(format!("display clear failed: {e:?}")))?;

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();

    Text::with_baseline("Hello", Point::new(0, 20), text_style, Baseline::Top)
        .draw(&mut display)
        .map_err(|e| std::io::Error::other(format!("text draw failed: {e:?}")))?;

    display
        .flush()
        .map_err(|e| std::io::Error::other(format!("display flush failed: {e:?}")))?;

    Ok(())
}