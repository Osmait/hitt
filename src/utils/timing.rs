use std::time::Duration;

pub fn format_timing_breakdown(
    dns: Duration,
    tcp: Duration,
    tls: Option<Duration>,
    ttfb: Duration,
    download: Duration,
    total: Duration,
) -> Vec<(String, Duration, f64)> {
    let total_nanos = total.as_nanos() as f64;
    let mut breakdown = vec![
        ("DNS Lookup".to_string(), dns, if total_nanos > 0.0 { dns.as_nanos() as f64 / total_nanos } else { 0.0 }),
        ("TCP Connect".to_string(), tcp, if total_nanos > 0.0 { tcp.as_nanos() as f64 / total_nanos } else { 0.0 }),
    ];

    if let Some(tls) = tls {
        breakdown.push(("TLS Handshake".to_string(), tls, if total_nanos > 0.0 { tls.as_nanos() as f64 / total_nanos } else { 0.0 }));
    }

    breakdown.push(("Time to First Byte".to_string(), ttfb, if total_nanos > 0.0 { ttfb.as_nanos() as f64 / total_nanos } else { 0.0 }));
    breakdown.push(("Content Download".to_string(), download, if total_nanos > 0.0 { download.as_nanos() as f64 / total_nanos } else { 0.0 }));
    breakdown.push(("Total".to_string(), total, 1.0));

    breakdown
}
