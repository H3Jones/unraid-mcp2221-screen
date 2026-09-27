use crate::types::MetricsSnapshot;

const DISPLAY_LINES: usize = 6;

trait InfoPage {
    fn build(&self, page_index: usize, total_pages: usize, metrics: &MetricsSnapshot) -> Vec<String>;
}

struct OverviewPage;

impl InfoPage for OverviewPage {
    fn build(&self, page_index: usize, total_pages: usize, metrics: &MetricsSnapshot) -> Vec<String> {
        normalize_page_lines(vec![
            build_header_line("Overview", page_index, total_pages),
            format!("IP     {}", metrics.ip_address),
            format!("RAM    {:.1}/{:.1} GiB", metrics.ram_used_gib, metrics.ram_max_gib),
            format!("Array  {:.0}% / {:.0} TB", metrics.array_used_pct, metrics.array_max_tb),
            format!("Cache  {:.0}% / {:.0} TB", metrics.cache_used_pct, metrics.cache_max_tb),
            format!("UP     {}", metrics.uptime),
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
            format!("Used   {:.1} GiB", metrics.ram_used_gib),
            format!("Total  {:.1} GiB", metrics.ram_max_gib),
            format!("Free   {:.1} GiB", ram_free),
            format!("Usage  {:.0}%", ram_pct),
            "Live memory data".to_string(),
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

        normalize_page_lines(vec![
            build_header_line(&format!("Storage:{}", storage.title), page_index, total_pages),
            format!("Used   {:.0}% / {:.1} TB", storage.used_pct, storage.max_tb),
            format!("Disks  {}", storage.disk_count),
            format!("Active {}", storage.active_count),
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