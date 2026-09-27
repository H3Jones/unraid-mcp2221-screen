use std::{
    env, io,
    net::ToSocketAddrs,
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::types::{MetricsSnapshot, StoragePageMetrics};

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const KIB_PER_TB: f64 = 1_000_000_000.0;
const UNRAID_QUERY: &str = r#"
query ScreenMetrics {
    server { lanip }
    info { os { uptime } }
    metrics { memory { total used } }
    array {
        capacity { kilobytes { total used } }
        disks { isSpinning }
        caches { name fsSize fsUsed isSpinning }
    }
}
"#;

pub struct UnraidClient {
    http: reqwest::Client,
    graphql_url: String,
    api_key: String,
}

#[derive(Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
struct ScreenMetricsData {
    server: ServerInfo,
    info: InfoRoot,
    metrics: MetricsRoot,
    array: ArrayRoot,
}

#[derive(Deserialize)]
struct ServerInfo {
    lanip: String,
}

#[derive(Deserialize)]
struct InfoRoot {
    os: InfoOs,
}

#[derive(Deserialize)]
struct InfoOs {
    uptime: String,
}

#[derive(Deserialize)]
struct MetricsRoot {
    memory: MemoryMetrics,
}

#[derive(Deserialize)]
struct MemoryMetrics {
    total: i64,
    used: i64,
}

#[derive(Deserialize)]
struct ArrayRoot {
    capacity: ArrayCapacityRoot,
    disks: Vec<ArrayDisk>,
    caches: Vec<CacheDisk>,
}

#[derive(Deserialize)]
struct ArrayCapacityRoot {
    kilobytes: CapacityTriplet,
}

#[derive(Deserialize)]
struct CapacityTriplet {
    total: String,
    used: String,
}

#[derive(Deserialize)]
struct CacheDisk {
    name: String,
    #[serde(rename = "fsSize")]
    fs_size: Option<i64>,
    #[serde(rename = "fsUsed")]
    fs_used: Option<i64>,
    #[serde(rename = "isSpinning")]
    is_spinning: Option<bool>,
}

#[derive(Deserialize)]
struct ArrayDisk {
    #[serde(rename = "isSpinning")]
    is_spinning: Option<bool>,
}

impl UnraidClient {
    pub fn from_env() -> io::Result<Self> {
        let graphql_url = select_graphql_url(&build_graphql_urls())?;

        let api_key = env::var("UNRAID_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| io::Error::other("Missing Unraid API key. Set UNRAID_API_KEY"))?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| io::Error::other(format!("http client init failed: {e}")))?;

        Ok(Self {
            http,
            graphql_url,
            api_key,
        })
    }

    pub async fn fetch_metrics_snapshot(&self) -> io::Result<MetricsSnapshot> {
        self.fetch_metrics_from_url(&self.graphql_url).await
    }

    async fn fetch_metrics_from_url(&self, graphql_url: &str) -> io::Result<MetricsSnapshot> {
        #[derive(serde::Serialize)]
        struct QueryBody<'a> {
            query: &'a str,
        }

        let response = self
            .http
            .post(graphql_url)
            .header("x-api-key", &self.api_key)
            .json(&QueryBody { query: UNRAID_QUERY })
            .send()
            .await
            .map_err(|e| io::Error::other(format!("graphql request failed: {e}")))?;

        let payload: GraphQlResponse<ScreenMetricsData> = response
            .json()
            .await
            .map_err(|e| io::Error::other(format!("graphql parse failed: {e}")))?;

        if let Some(errors) = payload.errors {
            let joined = errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(io::Error::other(format!("graphql error: {joined}")));
        }

        let data = payload
            .data
            .ok_or_else(|| io::Error::other("graphql response missing data"))?;

        Ok(map_graphql_to_snapshot(data))
    }
}

fn build_graphql_urls() -> Vec<String> {
    let mut urls = Vec::new();

    if let Ok(explicit) = env::var("UNRAID_GRAPHQL_URL")
        && let Some(url) = normalize_graphql_url(&explicit) {
            urls.push(url);
        }

    if let Ok(hostname) = env::var("HOST_HOSTNAME") {
        let host = hostname.trim().to_ascii_lowercase();
        if !host.is_empty() {
            if let Some(url) = normalize_graphql_url(&format!("http://{host}")) {
                urls.push(url);
            }
            if let Some(url) = normalize_graphql_url(&format!("http://{host}.local")) {
                urls.push(url);
            }
        }
    }

    if let Some(url) = normalize_graphql_url("http://tower.local") {
        urls.push(url);
    }

    let mut deduped = Vec::new();
    for url in urls {
        if !deduped.iter().any(|u| u == &url) {
            deduped.push(url);
        }
    }

    deduped
}

fn select_graphql_url(candidates: &[String]) -> io::Result<String> {
    if candidates.is_empty() {
        return Err(io::Error::other(
            "No GraphQL endpoint candidates found. Set UNRAID_GRAPHQL_URL or HOST_HOSTNAME",
        ));
    }

    let mut failures = Vec::new();
    for url in candidates {
        match resolve_url_host(url) {
            Ok(()) => {
                eprintln!("[unraid] selected graphql endpoint: {url}");
                return Ok(url.clone());
            }
            Err(err) => failures.push(format!("{url} => {err}")),
        }
    }

    Err(io::Error::other(format!(
        "No resolvable GraphQL endpoints. Tried: {}",
        failures.join(" | ")
    )))
}

fn resolve_url_host(url: &str) -> io::Result<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| io::Error::other(format!("invalid graphql url '{url}': {e}")))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| io::Error::other(format!("url has no host: {url}")))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| io::Error::other(format!("url has unknown port: {url}")))?;

    let mut addresses = (host, port)
        .to_socket_addrs()
        .map_err(|e| io::Error::other(format!("dns lookup failed for {host}:{port}: {e}")))?;

    if addresses.next().is_some() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "dns lookup returned no addresses for {host}:{port}"
        )))
    }
}

fn normalize_graphql_url(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.ends_with("/graphql") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/graphql"))
    }
}

fn map_graphql_to_snapshot(data: ScreenMetricsData) -> MetricsSnapshot {
    let ram_max_gib = bytes_to_gib(data.metrics.memory.total);
    let ram_used_gib = bytes_to_gib(data.metrics.memory.used);

    let array_total_kib = parse_kib_string(&data.array.capacity.kilobytes.total);
    let array_used_kib = parse_kib_string(&data.array.capacity.kilobytes.used);
    let array_used_pct = if array_total_kib > 0.0 {
        (array_used_kib / array_total_kib) * 100.0
    } else {
        0.0
    };
    let array_max_tb = array_total_kib / KIB_PER_TB;

    let (cache_used_kib, cache_max_kib) = data
        .array
        .caches
        .iter()
        .find(|disk| disk.name.eq_ignore_ascii_case("cache"))
        .and_then(|disk| Some((disk.fs_used?, disk.fs_size?)))
        .map(|(used, total)| (used as f64, total as f64))
        .unwrap_or((0.0, 0.0));

    let cache_used_pct = if cache_max_kib > 0.0 {
        (cache_used_kib / cache_max_kib) * 100.0
    } else {
        0.0
    };

    let array_disk_count = data.array.disks.len() as u32;
    let array_active_count = data
        .array
        .disks
        .iter()
        .filter(|disk| disk.is_spinning.unwrap_or(false))
        .count() as u32;

    let cache_disk_count = data.array.caches.len() as u32;
    let cache_active_count = data
        .array
        .caches
        .iter()
        .filter(|disk| disk.is_spinning.unwrap_or(false))
        .count() as u32;

    let storage_pages = vec![
        StoragePageMetrics {
            title: "Array".to_string(),
            used_pct: array_used_pct as f32,
            max_tb: (array_max_tb as f32).max(0.0),
            disk_count: array_disk_count,
            active_count: array_active_count,
        },
        StoragePageMetrics {
            title: "Cache".to_string(),
            used_pct: cache_used_pct as f32,
            max_tb: ((cache_max_kib / KIB_PER_TB) as f32).max(0.0),
            disk_count: cache_disk_count,
            active_count: cache_active_count,
        },
    ];

    MetricsSnapshot {
        ip_address: data.server.lanip,
        uptime: format_uptime(&data.info.os.uptime),
        ram_used_gib,
        ram_max_gib,
        array_used_pct: array_used_pct as f32,
        array_max_tb: (array_max_tb as f32).max(0.0),
        cache_used_pct: cache_used_pct as f32,
        cache_max_tb: ((cache_max_kib / KIB_PER_TB) as f32).max(0.0),
        storage_pages,
    }
}

fn parse_kib_string(value: &str) -> f64 {
    value.parse::<f64>().unwrap_or(0.0)
}

fn bytes_to_gib(value: i64) -> f32 {
    ((value as f64) / BYTES_PER_GIB) as f32
}

fn format_uptime(boot_timestamp: &str) -> String {
    match DateTime::parse_from_rfc3339(boot_timestamp) {
        Ok(boot_time) => {
            let duration = Utc::now().signed_duration_since(boot_time.with_timezone(&Utc));
            let total_hours = duration.num_hours().max(0);
            let days = total_hours / 24;
            let hours = total_hours % 24;
            format!("{}d {:02}h", days, hours)
        }
        Err(_) => "n/a".to_string(),
    }
}
