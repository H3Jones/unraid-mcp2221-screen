use std::{env, io, time::Duration};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::types::MetricsSnapshot;

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const KIB_PER_TB: f64 = 1_000_000_000.0;
const UNRAID_QUERY: &str = r#"
query ScreenMetrics {
    server { lanip }
    info { os { uptime } }
    metrics { memory { total used } }
    array {
        capacity { kilobytes { total used } }
        caches { name fsSize fsUsed }
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
}

impl UnraidClient {
    pub fn from_env() -> io::Result<Self> {
        let graphql_url =
            env::var("UNRAID_GRAPHQL_URL").unwrap_or_else(|_| "http://tower.local/graphql".to_string());

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
        #[derive(serde::Serialize)]
        struct QueryBody<'a> {
            query: &'a str,
        }

        let response = self
            .http
            .post(&self.graphql_url)
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

    MetricsSnapshot {
        ip_address: data.server.lanip,
        uptime: format_uptime(&data.info.os.uptime),
        ram_used_gib,
        ram_max_gib,
        array_used_pct: array_used_pct as f32,
        array_max_tb: (array_max_tb as f32).max(0.0),
        cache_used_pct: cache_used_pct as f32,
        cache_max_tb: ((cache_max_kib / KIB_PER_TB) as f32).max(0.0),
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
