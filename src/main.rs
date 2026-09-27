use std::{env, io, time::Duration};

mod types;
mod unraid;

use crate::types::MetricsSnapshot;
use crate::unraid::UnraidClient;
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
use tokio::time::{self, MissedTickBehavior};

const METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const BUTTON_POLL_INTERVAL: Duration = Duration::from_millis(100);

type OledDisplay = Ssd1306<I2CInterface<MCP2221>, DisplaySize128x64, BufferedGraphicsMode<DisplaySize128x64>>;

struct DisplayConnection {
    display: OledDisplay,
    address: u8,
    speed_bps: u32,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScreenPage {
    Overview,
    Memory,
    Storage,
}

impl ScreenPage {
    fn next(self) -> Self {
        match self {
            Self::Overview => Self::Memory,
            Self::Memory => Self::Storage,
            Self::Storage => Self::Overview,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Storage,
            Self::Memory => Self::Overview,
            Self::Storage => Self::Memory,
        }
    }
}

#[derive(Default)]
struct ButtonActions {
    next_page: bool,
    previous_page: bool,
}

struct ButtonReader {
    was_pressed: bool,
    warned: bool,
    configured: bool,
}

impl ButtonReader {
    fn new() -> Self {
        Self {
            was_pressed: false,
            warned: false,
            configured: false,
        }
    }

    fn poll(&mut self, mcp: &MCP2221) -> ButtonActions {
        // Active-low button on GP1 with press-edge detection.
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dry_run = env_flag_enabled("MCP2221_DRY_RUN");
    let auto_cycle_pages = env_flag_enabled("MCP2221_AUTO_CYCLE_PAGES");
    let enable_button = env_flag_enabled("MCP2221_ENABLE_BUTTON");

    if dry_run {
        run_dry_run_preview(auto_cycle_pages).await;
        return Ok(());
    }

    let unraid_client = match UnraidClient::from_env() {
        Ok(client) => Some(client),
        Err(err) => {
            eprintln!("Live metrics disabled: {err}. Falling back to representative stub values.");
            None
        }
    };

    let mut display_connection = connect_display()?;
    let mut button_reader = ButtonReader::new();
    let mut page = ScreenPage::Overview;
    let mut tick_counter: u64 = 0;
    let mut snapshot = fetch_metrics(tick_counter, unraid_client.as_ref()).await;

    let mut metrics_interval = time::interval(METRICS_REFRESH_INTERVAL);
    metrics_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut button_interval = time::interval(BUTTON_POLL_INTERVAL);
    button_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    render_metrics_page(&mut display_connection.display, page, &snapshot)?;

    loop {
        let mut should_render = false;

        tokio::select! {
            _ = metrics_interval.tick() => {
                tick_counter += 1;
                snapshot = fetch_metrics(tick_counter, unraid_client.as_ref()).await;

                let updated = update_page(
                    page,
                    ButtonActions::default(),
                    auto_cycle_pages,
                    tick_counter,
                );
                if updated != page {
                    page = updated;
                }

                should_render = true;
            }
            _ = button_interval.tick(), if enable_button => {
                let mcp = take_mcp_from_display(display_connection.display);
                let actions = button_reader.poll(&mcp);

                display_connection.display = match rewrap_display_from_mcp(
                    mcp,
                    display_connection.address,
                    display_connection.speed_bps,
                ) {
                    Ok(display) => display,
                    Err(err) => {
                        eprintln!("Display rebuild failed after button poll: {err}. Re-scanning...");
                        button_reader.configured = false;
                        let replacement = connect_display()?;
                        display_connection.address = replacement.address;
                        display_connection.speed_bps = replacement.speed_bps;
                        replacement.display
                    }
                };

                let next_page = update_page(page, actions, auto_cycle_pages, tick_counter);
                if next_page != page {
                    page = next_page;
                    should_render = true;
                }
            }
        }

        if should_render {
            if let Err(err) = render_metrics_page(&mut display_connection.display, page, &snapshot) {
                eprintln!("Display write failed: {err}. Reconnecting display...");
                button_reader.configured = false;
                display_connection = connect_display()?;
            }
        }
    }
}

async fn run_dry_run_preview(auto_cycle_pages: bool) {
    println!("Dry run enabled. Hardware access is skipped.");
    println!("Rendering preview text for local layout checks.");

    let mut page = ScreenPage::Overview;
    for tick in 1..=4 {
        let snapshot = fetch_metrics_stub(tick).await;
        let actions = poll_button_actions_stub();
        page = update_page(page, actions, auto_cycle_pages, tick);

        println!("--- Preview tick {tick} ---");
        for line in build_page_lines(page, &snapshot) {
            println!("{line}");
        }

        time::sleep(Duration::from_millis(500)).await;
    }
}

fn connect_display() -> io::Result<DisplayConnection> {
    let mut last_err: Option<io::Error> = None;

    for speed in [I2cSpeed::standard_100k(), I2cSpeed::new(50_000)] {
        for addr in [0x3C_u8, 0x3D_u8] {
            println!("Trying SSD1306 128x64 at 0x{addr:02X}, {} bps", speed.speed());

            match try_connect_display(addr, speed) {
                Ok(display) => {
                    println!("Connected to 128x64 at 0x{addr:02X}, {} bps", speed.speed());
                    return Ok(DisplayConnection {
                        display,
                        address: addr,
                        speed_bps: speed.speed(),
                    });
                }
                Err(e) => {
                    println!("Failed on 0x{addr:02X} at {} bps: {e}", speed.speed());
                    last_err = Some(e);
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::other("SSD1306 128x64 connect test failed")))
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

async fn fetch_metrics_stub(tick: u64) -> MetricsSnapshot {
    let wiggle = (tick % 6) as f32;
    let array_used = 61.0 + wiggle;
    let cache_used = 33.0 + (wiggle * 1.8);

    MetricsSnapshot {
        ip_address: "192.168.1.188".to_string(),
        uptime: format!("{}d {:02}h", 12 + (tick / 12), (tick * 3) % 24),
        ram_used_gib: 11.4 + wiggle,
        ram_max_gib: 31.2,
        array_used_pct: array_used.min(95.0),
        array_max_tb: 36.0,
        cache_used_pct: cache_used.min(92.0),
        cache_max_tb: 2.0,
    }
}

async fn fetch_metrics(tick: u64, client: Option<&UnraidClient>) -> MetricsSnapshot {
    let Some(client) = client else {
        return fetch_metrics_stub(tick).await;
    };

    match client.fetch_metrics_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            eprintln!("Live metrics fetch failed: {err}. Using fallback values.");
            fetch_metrics_stub(tick).await
        }
    }
}
fn poll_button_actions_stub() -> ButtonActions {
    // Placeholder for future MCP2221 GPIO button reads.
    ButtonActions::default()
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
    Ok(Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0).into_buffered_graphics_mode())
}

fn update_page(current: ScreenPage, actions: ButtonActions, auto_cycle_pages: bool, tick: u64) -> ScreenPage {
    if actions.next_page {
        return current.next();
    }

    if actions.previous_page {
        return current.previous();
    }

    if auto_cycle_pages && tick % 3 == 0 {
        return current.next();
    }

    current
}

fn render_metrics_page(display: &mut OledDisplay, page: ScreenPage, metrics: &MetricsSnapshot) -> io::Result<()> {
    display
        .clear(BinaryColor::Off)
        .map_err(|e| io::Error::other(format!("display clear failed: {e:?}")))?;

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    for (index, line) in build_page_lines(page, metrics).iter().enumerate() {
        Text::with_baseline(line, Point::new(0, (index as i32) * 10), text_style, Baseline::Top)
            .draw(display)
            .map_err(|e| io::Error::other(format!("text draw failed: {e:?}")))?;
    }

    display
        .flush()
        .map_err(|e| io::Error::other(format!("display flush failed: {e:?}")))
}

fn build_page_lines(page: ScreenPage, metrics: &MetricsSnapshot) -> Vec<String> {
    match page {
        ScreenPage::Overview => vec![
            "Unraid Status".to_string(),
            format!("IP     {}", metrics.ip_address),
            format!("RAM    {:.1}/{:.1} GiB", metrics.ram_used_gib, metrics.ram_max_gib),
            format!("Array  {:.0}% of {:.0} TB", metrics.array_used_pct, metrics.array_max_tb),
            format!("Cache  {:.0}% of {:.0} TB", metrics.cache_used_pct, metrics.cache_max_tb),
            format!("UP     {}", metrics.uptime),
        ],
        ScreenPage::Memory => vec![
            "Memory".to_string(),
            format!("Used   {:.1} GiB", metrics.ram_used_gib),
            format!("Total  {:.1} GiB", metrics.ram_max_gib),
            format!("Free   {:.1} GiB", (metrics.ram_max_gib - metrics.ram_used_gib).max(0.0)),
            format!("IP     {}", metrics.ip_address),
            format!("UP     {}", metrics.uptime),
        ],
        ScreenPage::Storage => vec![
            "Storage".to_string(),
            format!("Array  {:.0}% / {:.0} TB", metrics.array_used_pct, metrics.array_max_tb),
            format!("Cache  {:.0}% / {:.0} TB", metrics.cache_used_pct, metrics.cache_max_tb),
            format!("RAM    {:.1}/{:.1} GiB", metrics.ram_used_gib, metrics.ram_max_gib),
            format!("IP     {}", metrics.ip_address),
            format!("UP     {}", metrics.uptime),
        ],
    }
}

fn try_connect_display(addr: u8, speed: I2cSpeed) -> Result<OledDisplay, io::Error> {
    let mcp = MCP2221::connect().map_err(|e| io::Error::other(format!("connect failed: {e:?}")))?;

    mcp.i2c_set_bus_speed(speed)
        .map_err(|e| io::Error::other(format!("set bus speed failed: {e:?}")))?;

    let _ = mcp.i2c_cancel_transfer();

    let interface = I2CDisplayInterface::new_custom_address(mcp, addr);
    let mut display =
        Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0).into_buffered_graphics_mode();

    display
        .init()
        .map_err(|e| io::Error::other(format!("display init failed: {e:?}")))?;

    Ok(display)
}