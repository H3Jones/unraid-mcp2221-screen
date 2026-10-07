use std::io;

use crate::pages::build_page_lines;
use crate::types::MetricsSnapshot;
use display_interface_i2c::I2CInterface;
use embedded_graphics::{
    mono_font::{MonoTextStyleBuilder, ascii::FONT_6X10},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use mcp2221_hal::gpio::{Input, Pins};
use mcp2221_hal::{MCP2221, i2c::I2cSpeed};
use ssd1306::{
    I2CDisplayInterface, Ssd1306,
    mode::{BufferedGraphicsMode, DisplayConfig},
    prelude::{DisplayRotation, DisplaySize128x64},
};

type OledDisplay =
    Ssd1306<I2CInterface<MCP2221>, DisplaySize128x64, BufferedGraphicsMode<DisplaySize128x64>>;

pub struct DisplayConnection {
    display: OledDisplay,
    address: u8,
    speed_bps: u32,
}

#[derive(Default)]
pub struct ButtonActions {
    pub next_page: bool,
    pub previous_page: bool,
}

pub struct ButtonReader {
    was_pressed: bool,
    warned: bool,
    configured: bool,
}

impl ButtonReader {
    pub fn new() -> Self {
        Self {
            was_pressed: false,
            warned: false,
            configured: false,
        }
    }

    pub fn reset(&mut self) {
        self.configured = false;
    }

    fn poll(&mut self, mcp: &MCP2221) -> ButtonActions {
        let pressed_now = match read_button_gp1_pressed(mcp, &mut self.configured) {
            Ok(value) => {
                self.warned = false;
                value
            }
            Err(err) => {
                if !self.warned {
                    eprintln!("Button read disabled: {err}");
                    self.warned = true;
                }
                false
            }
        };

        let actions = ButtonActions {
            next_page: pressed_now && !self.was_pressed,
            previous_page: false,
        };

        if actions.next_page {
            eprintln!("[trace] GP1 button press detected, cycling to next page");
        }

        self.was_pressed = pressed_now;
        actions
    }
}

impl DisplayConnection {
    pub fn connect() -> io::Result<Self> {
        let mut last_err: Option<io::Error> = None;

        for speed in [I2cSpeed::standard_100k(), I2cSpeed::new(50_000)] {
            for addr in [0x3C_u8, 0x3D_u8] {
                println!(
                    "Trying SSD1306 128x64 at 0x{addr:02X}, {} bps",
                    speed.speed()
                );

                match try_connect_display(addr, speed) {
                    Ok(display) => {
                        println!("Connected to 128x64 at 0x{addr:02X}, {} bps", speed.speed());
                        return Ok(Self {
                            display,
                            address: addr,
                            speed_bps: speed.speed(),
                        });
                    }
                    Err(err) => {
                        println!("Failed on 0x{addr:02X} at {} bps: {err}", speed.speed());
                        last_err = Some(err);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| io::Error::other("SSD1306 128x64 connect test failed")))
    }

    pub fn poll_button(mut self, reader: &mut ButtonReader) -> io::Result<(Self, ButtonActions)> {
        let mcp = take_mcp_from_display(self.display);
        let actions = reader.poll(&mcp);

        self.display = match rewrap_display_from_mcp(mcp, self.address, self.speed_bps) {
            Ok(display) => display,
            Err(err) => {
                eprintln!("Display rebuild failed after button poll: {err}. Re-scanning...");
                reader.reset();
                let replacement = Self::connect()?;
                self.address = replacement.address;
                self.speed_bps = replacement.speed_bps;
                replacement.display
            }
        };

        Ok((self, actions))
    }

    pub fn render(
        &mut self,
        page_index: usize,
        metrics: &MetricsSnapshot,
        error: Option<&str>,
    ) -> io::Result<()> {
        if let Some(error) = error {
            return render_error(&mut self.display, error);
        }

        render_metrics_page(&mut self.display, page_index, metrics)
    }
}

fn read_button_gp1_pressed(device: &MCP2221, configured: &mut bool) -> io::Result<bool> {
    if !*configured {
        let Pins { gp1, .. } = device
            .gpio_take_pins()
            .ok_or_else(|| io::Error::other("button pins already taken"))?;

        let _button_pin: Input<'_> = gp1
            .try_into()
            .map_err(|e| io::Error::other(format!("button gp1 input config failed: {e:?}")))?;

        *configured = true;
    }

    let values = device
        .gpio_read()
        .map_err(|e| io::Error::other(format!("button gpio_read failed: {e:?}")))?;

    let (direction, level) = values
        .gp1
        .ok_or_else(|| io::Error::other("button gp1 is not in GPIO mode"))?;

    if !direction.is_input() {
        return Err(io::Error::other("button gp1 is not configured as input"));
    }

    Ok(level.is_low())
}

fn take_mcp_from_display(display: OledDisplay) -> MCP2221 {
    display.release().release()
}

fn rewrap_display_from_mcp(mcp: MCP2221, addr: u8, speed_bps: u32) -> io::Result<OledDisplay> {
    mcp.i2c_set_bus_speed(I2cSpeed::new(speed_bps))
        .map_err(|e| io::Error::other(format!("set bus speed failed: {e:?}")))?;

    let interface = I2CDisplayInterface::new_custom_address(mcp, addr);
    Ok(
        Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate180)
            .into_buffered_graphics_mode(),
    )
}

fn render_metrics_page(
    display: &mut OledDisplay,
    page_index: usize,
    metrics: &MetricsSnapshot,
) -> io::Result<()> {
    display
        .clear(BinaryColor::Off)
        .map_err(|e| io::Error::other(format!("display clear failed: {e:?}")))?;

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    for (index, line) in build_page_lines(page_index, metrics).iter().enumerate() {
        Text::with_baseline(
            line,
            Point::new(0, (index as i32) * 10),
            text_style,
            Baseline::Top,
        )
        .draw(display)
        .map_err(|e| io::Error::other(format!("text draw failed: {e:?}")))?;
    }

    display
        .flush()
        .map_err(|e| io::Error::other(format!("display flush failed: {e:?}")))
}

fn render_error(display: &mut OledDisplay, error: &str) -> io::Result<()> {
    display
        .clear(BinaryColor::Off)
        .map_err(|e| io::Error::other(format!("display clear failed: {e:?}")))?;

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();
    let error_chars = error.chars().collect::<Vec<_>>();
    let lines = std::iter::once("Metrics unavailable".to_string())
        .chain(error_chars.chunks(21).map(|chunk| chunk.iter().collect()))
        .take(6);

    for (index, line) in lines.enumerate() {
        Text::with_baseline(
            &line,
            Point::new(0, (index as i32) * 10),
            text_style,
            Baseline::Top,
        )
        .draw(display)
        .map_err(|e| io::Error::other(format!("text draw failed: {e:?}")))?;
    }

    display
        .flush()
        .map_err(|e| io::Error::other(format!("display flush failed: {e:?}")))
}

fn try_connect_display(addr: u8, speed: I2cSpeed) -> Result<OledDisplay, io::Error> {
    let mcp = MCP2221::connect().map_err(|e| io::Error::other(format!("connect failed: {e:?}")))?;

    mcp.i2c_set_bus_speed(speed)
        .map_err(|e| io::Error::other(format!("set bus speed failed: {e:?}")))?;

    let _ = mcp.i2c_cancel_transfer();

    let interface = I2CDisplayInterface::new_custom_address(mcp, addr);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate180)
        .into_buffered_graphics_mode();

    display
        .init()
        .map_err(|e| io::Error::other(format!("display init failed: {e:?}")))?;

    Ok(display)
}
