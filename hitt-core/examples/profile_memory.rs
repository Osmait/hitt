#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use hitt_core::core::client::HttpClient;
use hitt_core::core::request::{HttpMethod, Request};
use hitt_core::core::variables::VariableResolver;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let rt = tokio::runtime::Runtime::new().unwrap();

    let client = HttpClient::new().unwrap();
    let resolver = VariableResolver::new();

    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://httpbin.org/get".into());

    let request = Request::new("profile", HttpMethod::GET, &url);

    println!("Profiling single GET to {url}...");
    let response = rt.block_on(client.send(&request, &resolver));

    match response {
        Ok(resp) => println!("Status: {} — body {} bytes", resp.status, resp.size.body),
        Err(e) => eprintln!("Error: {e}"),
    }

    // dhat profiler drops here, printing the heap report
}
