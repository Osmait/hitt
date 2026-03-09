use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::core::client::HttpClient;
use crate::core::request::Request;
use crate::core::variables::VariableResolver;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestConfig {
    pub request_id: Uuid,
    pub total_requests: u32,
    pub concurrency: u32,
    pub ramp_up_seconds: Option<u32>,
    pub timeout_ms: u64,
}

impl LoadTestConfig {
    pub fn new(request_id: Uuid, total_requests: u32, concurrency: u32) -> Self {
        Self {
            request_id,
            total_requests,
            concurrency,
            ramp_up_seconds: None,
            timeout_ms: 30000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadTestResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub error_rate: f64,
    pub duration: Duration,
    pub rps: f64,
    pub latency: LatencyStats,
    pub status_distribution: HashMap<u16, u32>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub median: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub std_dev: Duration,
}

impl LatencyStats {
    fn from_durations(mut durations: Vec<Duration>) -> Self {
        if durations.is_empty() {
            return Self {
                min: Duration::ZERO,
                max: Duration::ZERO,
                mean: Duration::ZERO,
                median: Duration::ZERO,
                p90: Duration::ZERO,
                p95: Duration::ZERO,
                p99: Duration::ZERO,
                std_dev: Duration::ZERO,
            };
        }

        durations.sort();
        let len = durations.len();
        let total: Duration = durations.iter().sum();
        let mean = total / len as u32;

        let median = durations[len / 2];
        let p90 = durations[(len as f64 * 0.90) as usize];
        let p95 = durations[(len as f64 * 0.95) as usize];
        let p99 = durations[((len as f64 * 0.99) as usize).min(len - 1)];

        // Standard deviation
        let mean_nanos = mean.as_nanos() as f64;
        let variance: f64 = durations
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - mean_nanos;
                diff * diff
            })
            .sum::<f64>()
            / len as f64;
        let std_dev = Duration::from_nanos(variance.sqrt() as u64);

        Self {
            min: durations[0],
            max: durations[len - 1],
            mean,
            median,
            p90,
            p95,
            p99,
            std_dev,
        }
    }
}

#[derive(Debug, Clone)]
struct RequestResult {
    status: Option<u16>,
    duration: Duration,
    error: Option<String>,
}

pub async fn run_load_test(
    config: &LoadTestConfig,
    request: &Request,
    resolver: &VariableResolver,
) -> Result<LoadTestResult> {
    let client = HttpClient::new()?;
    let results = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();

    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.concurrency as usize));
    let mut handles = Vec::new();

    for _ in 0..config.total_requests {
        let permit = semaphore.clone().acquire_owned().await?;
        let client_clone = HttpClient::new()?;
        let request_clone = request.clone();
        let resolver_clone = VariableResolver::new(); // simplified for load test
        let results_clone = results.clone();
        let timeout = Duration::from_millis(config.timeout_ms);

        handles.push(tokio::spawn(async move {
            let req_start = Instant::now();
            let result = tokio::time::timeout(
                timeout,
                client_clone.send(&request_clone, &resolver_clone),
            )
            .await;

            let duration = req_start.elapsed();
            let req_result = match result {
                Ok(Ok(resp)) => RequestResult {
                    status: Some(resp.status),
                    duration,
                    error: None,
                },
                Ok(Err(e)) => RequestResult {
                    status: None,
                    duration,
                    error: Some(e.to_string()),
                },
                Err(_) => RequestResult {
                    status: None,
                    duration,
                    error: Some("Request timed out".to_string()),
                },
            };

            results_clone.lock().await.push(req_result);
            drop(permit);
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let total_duration = start.elapsed();
    let results = results.lock().await;

    let successful = results.iter().filter(|r| r.error.is_none()).count() as u32;
    let failed = results.iter().filter(|r| r.error.is_some()).count() as u32;
    let total = config.total_requests;

    let mut status_distribution = HashMap::new();
    for r in results.iter() {
        if let Some(status) = r.status {
            *status_distribution.entry(status).or_insert(0u32) += 1;
        }
    }

    let durations: Vec<Duration> = results.iter().map(|r| r.duration).collect();
    let latency = LatencyStats::from_durations(durations);

    let errors: Vec<String> = results
        .iter()
        .filter_map(|r| r.error.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    Ok(LoadTestResult {
        total_requests: total,
        successful,
        failed,
        error_rate: if total > 0 {
            failed as f64 / total as f64 * 100.0
        } else {
            0.0
        },
        duration: total_duration,
        rps: if total_duration.as_secs_f64() > 0.0 {
            total as f64 / total_duration.as_secs_f64()
        } else {
            0.0
        },
        latency,
        status_distribution,
        errors,
    })
}
