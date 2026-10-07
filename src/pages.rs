use crate::types::MetricsSnapshot;

const DISPLAY_LINES: usize = 6;
const LABEL_WIDTH: usize = 6;

macro_rules! metric_line {
    ($label:expr, $($arg:tt)*) => {
        format!("{:<width$} {}", $label, format!($($arg)*), width = LABEL_WIDTH)
    };
}

trait InfoPage {
    fn build(&self, page_index: usize, total_pages: usize, metrics: &MetricsSnapshot) -> Vec<String>;
}

struct OverviewPage;

impl InfoPage for OverviewPage {
    fn build(&self, page_index: usize, total_pages: usize, metrics: &MetricsSnapshot) -> Vec<String> {
        let array_used_tb = metrics.array_used_pct / 100.0 * metrics.array_max_tb;
        let cache_used_tb = metrics.cache_used_pct / 100.0 * metrics.cache_max_tb;

        normalize_page_lines(vec![
            build_header_line("Overview", page_index, total_pages),
            metric_line!("IP", "{}", metrics.ip_address),
            metric_line!("RAM", "{:.1}/{:.1} GiB", metrics.ram_used_gib, metrics.ram_max_gib),
            metric_line!("Array", "{:.1}/{:.1} TB", array_used_tb, metrics.array_max_tb),
            metric_line!("Cache", "{:.1}/{:.1} TB", cache_used_tb, metrics.cache_max_tb),
            metric_line!("Uptime", "{}", metrics.uptime),
        ])
    }
}

struct MemoryPage;

impl InfoPage for MemoryPage {
    fn build(&self, page_index: usize, total_pages: usize, metrics: &MetricsSnapshot) -> Vec<String> {
        let ram_free = (metrics.ram_max_gib - metrics.ram_used_gib).max(0.0);
        let ram_pct = if metrics.ram_max_gib > 0.0 {
            (metrics.ram_used_gib / metrics.ram_max_gib) * 100.0
        } else {
            0.0
        };

        normalize_page_lines(vec![
            build_header_line("Memory", page_index, total_pages),
            metric_line!("Used", "{:.1} GiB", metrics.ram_used_gib),
            metric_line!("Total", "{:.1} GiB", metrics.ram_max_gib),
            metric_line!("Free", "{:.1} GiB", ram_free),
            metric_line!("Usage", "{:.0}%", ram_pct),
        ])
    }
}

struct StoragePage {
    storage_index: usize,
}

impl InfoPage for StoragePage {
    fn build(&self, page_index: usize, total_pages: usize, metrics: &MetricsSnapshot) -> Vec<String> {
        let Some(storage) = metrics.storage_pages.get(self.storage_index) else {
            return normalize_page_lines(vec![
                build_header_line("Storage", page_index, total_pages),
                "No storage page data".to_string(),
            ]);
        };
        let used_tb = storage.used_pct / 100.0 * storage.max_tb;

        normalize_page_lines(vec![
            build_header_line(&format!("Storage:{}", storage.title), page_index, total_pages),
            metric_line!("Used", "{:.1}/{:.1} TB", used_tb, storage.max_tb),
            metric_line!("Disks", "{}", storage.disk_count),
            metric_line!("Active", "{}", storage.active_count),
        ])
    }
}

pub fn build_page_lines(page_index: usize, metrics: &MetricsSnapshot) -> Vec<String> {
    let pages = build_pages(metrics);
    let total = pages.len().max(1);
    let idx = page_index.min(total.saturating_sub(1));

    pages
        .get(idx)
        .map(|page| page.build(idx, total, metrics))
        .unwrap_or_else(|| {
            normalize_page_lines(vec![
                build_header_line("No Page", 0, 1),
                "No page builders registered".to_string(),
            ])
        })
}

pub fn total_pages(metrics: &MetricsSnapshot) -> usize {
    build_pages(metrics).len().max(1)
}

fn build_pages(metrics: &MetricsSnapshot) -> Vec<Box<dyn InfoPage>> {
    let mut pages: Vec<Box<dyn InfoPage>> = vec![Box::new(OverviewPage), Box::new(MemoryPage)];

    for storage_index in 0..metrics.storage_pages.len() {
        pages.push(Box::new(StoragePage { storage_index }));
    }

    pages
}

/// Ensures the page has exactly `DISPLAY_LINES` lines, padding with empty lines if necessary.
fn normalize_page_lines(mut lines: Vec<String>) -> Vec<String> {
    lines.truncate(DISPLAY_LINES);
    while lines.len() < DISPLAY_LINES {
        lines.push(String::new());
    }
    lines
}

fn build_header_line(title: &str, page_index: usize, total_pages: usize) -> String {
    let indicator = format!("{}/{}", page_index + 1, total_pages.max(1));
    let max_chars: usize = 21;
    let max_title = max_chars.saturating_sub(indicator.len() + 1);
    let trimmed = title.chars().take(max_title).collect::<String>();
    let spacing = max_chars.saturating_sub(trimmed.len() + indicator.len());
    format!("{trimmed}{:spacing$}{indicator}", "", spacing = spacing)
}

#[cfg(test)]
mod tests {
    use super::{build_page_lines, total_pages, DISPLAY_LINES};
    use crate::types::{MetricsSnapshot, StoragePageMetrics};

    #[test]
    fn pages_have_six_lines_and_out_of_range_indexes_clamp_to_last_page() {
        let metrics = MetricsSnapshot {
            ip_address: "192.168.1.188".to_string(),
            uptime: "1d 02h".to_string(),
            ram_used_gib: 8.0,
            ram_max_gib: 16.0,
            array_used_pct: 50.0,
            array_max_tb: 20.0,
            cache_used_pct: 25.0,
            cache_max_tb: 2.0,
            storage_pages: vec![StoragePageMetrics {
                title: "Array".to_string(),
                used_pct: 50.0,
                max_tb: 20.0,
                disk_count: 4,
                active_count: 2,
            }],
        };

        assert_eq!(total_pages(&metrics), 3);
        for page_index in 0..total_pages(&metrics) {
            assert_eq!(build_page_lines(page_index, &metrics).len(), DISPLAY_LINES);
        }

        let last_page = build_page_lines(2, &metrics);
        assert!(last_page[0].starts_with("Storage:Array"));
        assert_eq!(build_page_lines(usize::MAX, &metrics), last_page);
    }
}