#[derive(Clone, Debug)]
pub struct MetricsSnapshot {
    pub ip_address: String,
    pub uptime: String,
    pub ram_used_gib: f32,
    pub ram_max_gib: f32,
    pub array_used_pct: f32,
    pub array_max_tb: f32,
    pub cache_used_pct: f32,
    pub cache_max_tb: f32,
}
