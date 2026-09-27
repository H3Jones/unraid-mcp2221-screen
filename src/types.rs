#[derive(Clone, Debug)]
/// Metrics for a single storage page.
pub struct StoragePageMetrics {
    pub title: String,
    pub used_pct: f32,
    pub max_tb: f32,
    pub disk_count: u32,
    pub active_count: u32,
}

#[derive(Clone, Debug)]
/// Snapshot of the current system metrics.
pub struct MetricsSnapshot {
    pub ip_address: String,
    pub uptime: String,
    pub ram_used_gib: f32,
    pub ram_max_gib: f32,
    pub array_used_pct: f32,
    pub array_max_tb: f32,
    pub cache_used_pct: f32,
    pub cache_max_tb: f32,
    pub storage_pages: Vec<StoragePageMetrics>,
}
