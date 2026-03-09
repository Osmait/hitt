use hitt::core::client::HttpClient;
use hitt::core::request::{HttpMethod, Request};
use hitt::core::variables::VariableResolver;

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = HttpClient::new().unwrap();
    let resolver = VariableResolver::new();

    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://httpbin.org/get".into());
    let n: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let request = Request::new("profile", HttpMethod::GET, &url);

    println!("Profiling {n} sequential GET requests to {url}...");
    let start = std::time::Instant::now();

    rt.block_on(async {
        for i in 1..=n {
            match client.send(&request, &resolver).await {
                Ok(resp) => {
                    if i == 1 || i == n {
                        println!("  [{i}/{n}] status={} body={} bytes", resp.status, resp.size.body);
                    }
                }
                Err(e) => eprintln!("  [{i}/{n}] error: {e}"),
            }
        }
    });

    let elapsed = start.elapsed();
    println!("Done in {elapsed:.2?} ({:.1} req/s)", n as f64 / elapsed.as_secs_f64());
}
