use std::{env, time::Duration};

mod display;
mod pages;
mod types;
mod unraid;

use crate::display::{ButtonActions, ButtonReader, DisplayConnection};
use crate::pages::total_pages;
use crate::types::{MetricsSnapshot, StoragePageMetrics};
use crate::unraid::UnraidClient;
use tokio::time::{self, MissedTickBehavior};

const METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const BUTTON_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let auto_cycle_pages = env_flag_enabled("MCP2221_AUTO_CYCLE_PAGES");
    let enable_button = env_flag_enabled_or_default("MCP2221_ENABLE_BUTTON", true);
    let show_stub = env_flag_enabled("MCP2221_SHOW_STUB");

    let mut unraid_client = match UnraidClient::from_env() {
        Ok(client) => Some(client),
        Err(err) => {
            eprintln!("Live metrics unavailable: {err}");
            None
        }
    };

    let mut display_connection = DisplayConnection::connect()?;
    let mut button_reader = ButtonReader::new();
    let mut page_index: usize = 0;
    let mut tick_counter: u64 = 0;
    let (mut snapshot, mut metrics_error) =
        load_metrics(tick_counter, unraid_client.as_mut(), show_stub).await;

    let mut metrics_interval = time::interval(METRICS_REFRESH_INTERVAL);
    metrics_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut button_interval = time::interval(BUTTON_POLL_INTERVAL);
    button_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    display_connection.render(page_index, &snapshot, metrics_error.as_deref())?;

    loop {
        let mut should_render = false;

        tokio::select! {
            _ = metrics_interval.tick() => {
                tick_counter += 1;
                (snapshot, metrics_error) =
                    load_metrics(tick_counter, unraid_client.as_mut(), show_stub).await;

                let updated = update_page(
                    page_index,
                    ButtonActions::default(),
                    auto_cycle_pages,
                    tick_counter,
                    total_pages(&snapshot),
                );
                if updated != page_index {
                    page_index = updated;
                }

                should_render = true;
            }
            _ = button_interval.tick(), if enable_button => {
                let (replacement, actions) = display_connection.poll_button(&mut button_reader)?;
                display_connection = replacement;

                let next_page = update_page(page_index, actions, auto_cycle_pages, tick_counter, total_pages(&snapshot));
                if next_page != page_index {
                    page_index = next_page;
                    should_render = true;
                }
            }
        }

        page_index = page_index.min(total_pages(&snapshot).saturating_sub(1));

        if should_render
            && let Err(err) =
                display_connection.render(page_index, &snapshot, metrics_error.as_deref())
        {
            eprintln!("Display write failed: {err}. Reconnecting display...");
            button_reader.reset();
            display_connection = DisplayConnection::connect()?;
        }
    }
}

fn env_flag_enabled(name: &str) -> bool {
    env_flag_enabled_or_default(name, false)
}

fn env_flag_enabled_or_default(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            match v.as_str() {
                "1" | "true" | "yes" | "on" => true,
                "0" | "false" | "no" | "off" => false,
                _ => default,
            }
        }
        Err(_) => default,
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
        storage_pages: vec![
            StoragePageMetrics {
                title: "Array".to_string(),
                used_pct: array_used.min(95.0),
                max_tb: 36.0,
                disk_count: 12,
                active_count: 8,
            },
            StoragePageMetrics {
                title: "Cache".to_string(),
                used_pct: cache_used.min(92.0),
                max_tb: 2.0,
                disk_count: 2,
                active_count: 1,
            },
        ],
    }
}

async fn fetch_metrics(client: Option<&mut UnraidClient>) -> Result<MetricsSnapshot, String> {
    let Some(client) = client else {
        return Err("Unraid API client is not configured".to_string());
    };

    match client.fetch_metrics_snapshot().await {
        Ok(snapshot) => Ok(snapshot),
        Err(err) => Err(err.to_string()),
    }
}

async fn load_metrics(
    tick: u64,
    client: Option<&mut UnraidClient>,
    show_stub: bool,
) -> (MetricsSnapshot, Option<String>) {
    match fetch_metrics(client).await {
        Ok(snapshot) => (snapshot, None),
        Err(err) if show_stub => {
            eprintln!("Live metrics unavailable: {err}. Showing stub values.");
            (fetch_metrics_stub(tick).await, None)
        }
        Err(err) => {
            eprintln!("Live metrics unavailable: {err}");
            (fetch_metrics_stub(tick).await, Some(err))
        }
    }
}

fn update_page(
    current: usize,
    actions: ButtonActions,
    auto_cycle_pages: bool,
    tick: u64,
    total_pages: usize,
) -> usize {
    let total = total_pages.max(1);

    if actions.next_page {
        return (current + 1) % total;
    }

    if actions.previous_page {
        return (current + total - 1) % total;
    }

    if auto_cycle_pages && tick.is_multiple_of(3) {
        return (current + 1) % total;
    }

    current
}
