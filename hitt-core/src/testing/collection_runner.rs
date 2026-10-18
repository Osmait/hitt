//! Collection runner: send every request of a collection (or one folder)
//! sequentially, run each request's assertions and build a report.
//!
//! Requests are visited in tree order (folders recursively). Variables are
//! resolved with the active environment overriding collection variables.
//! WebSocket / SSE / gRPC requests are reported as skipped.
//!
//! Auth inheritance follows Postman semantics: a request whose auth is unset
//! or [`AuthConfig::Inherit`] uses the auth of its nearest folder that sets
//! one, falling back to the collection auth. [`AuthConfig::None`] means
//! "explicitly no auth" and is never overridden.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::core::auth::AuthConfig;
use crate::core::client::HttpClient;
use crate::core::collection::{Collection, CollectionItem};
use crate::core::environment::Environment;
use crate::core::helpers::append_query_params;
use crate::core::request::{Protocol, Request};
use crate::core::response::Response;
use crate::core::variables::VariableResolver;
use crate::testing::assertion_engine::AssertionEngine;

// ---------------------------------------------------------------------------
// Options / plan
// ---------------------------------------------------------------------------

/// Settings for a collection run.
#[derive(Debug, Clone)]
pub struct RunnerOptions {
    /// Only run this folder: matched by name, `/`-separated path
    /// (case-insensitive) or folder id.
    pub folder: Option<String>,
    /// Stop after the first failed or errored request.
    pub stop_on_failure: bool,
    /// Pause between two consecutive requests.
    pub delay_ms: u64,
    /// Run the whole plan this many times. Must be at least 1.
    pub iterations: u32,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            folder: None,
            stop_on_failure: false,
            delay_ms: 0,
            iterations: 1,
        }
    }
}

/// A request scheduled by the runner, with its inherited auth already applied.
#[derive(Debug, Clone)]
pub struct PlannedRequest {
    pub request: Request,
    /// `/`-separated folder path inside the collection (`None` at the root).
    pub folder: Option<String>,
}

/// List the requests a run would execute, in tree order.
///
/// Fails if `folder` is given but no folder matches it.
pub fn plan_requests(collection: &Collection, folder: Option<&str>) -> Result<Vec<PlannedRequest>> {
    let root_auth = explicit_auth(collection.auth.as_ref());
    let mut out = Vec::new();
    match folder.map(str::trim).filter(|f| !f.is_empty()) {
        None => collect_planned(&collection.items, &mut Vec::new(), root_auth, &mut out),
        Some(selector) => {
            let mut path = Vec::new();
            let (items, auth) = find_folder(&collection.items, selector, &mut path, root_auth)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Folder '{selector}' not found in collection '{}'",
                        collection.name
                    )
                })?;
            collect_planned(items, &mut path, auth, &mut out);
        }
    }
    Ok(out)
}

/// Full `/`-separated path of the folder matching `selector` (name, path or id).
pub fn resolve_folder_path(collection: &Collection, selector: &str) -> Option<String> {
    let mut path = Vec::new();
    find_folder(&collection.items, selector.trim(), &mut path, None)?;
    Some(path.join("/"))
}

/// The auth a request is sent with, given the auth inherited from its parents.
pub fn effective_auth(
    own: Option<&AuthConfig>,
    inherited: Option<&AuthConfig>,
) -> Option<AuthConfig> {
    match own {
        None | Some(AuthConfig::Inherit) => inherited.cloned(),
        Some(auth) => Some(auth.clone()),
    }
}

/// The auth `request` is sent with inside `collection`: its own auth, or the
/// auth inherited from its nearest folder / the collection when it has none
/// or is set to `Inherit`. Used for single sends so they match the runner.
pub fn effective_auth_in(collection: &Collection, request: &Request) -> Option<AuthConfig> {
    let root = explicit_auth(collection.auth.as_ref());
    let inherited = inherited_auth(&collection.items, &request.id, root).flatten();
    effective_auth(request.auth.as_ref(), inherited)
}

/// Auth inherited by the request `id`: `None` if the request isn't found,
/// `Some(None)` if found with nothing to inherit.
fn inherited_auth<'a>(
    items: &'a [CollectionItem],
    id: &Uuid,
    inherited: Option<&'a AuthConfig>,
) -> Option<Option<&'a AuthConfig>> {
    for item in items {
        match item {
            CollectionItem::Request(req) if req.id == *id => return Some(inherited),
            CollectionItem::Request(_) => {}
            CollectionItem::Folder { items, auth, .. } => {
                let next = explicit_auth(auth.as_ref()).or(inherited);
                if let Some(found) = inherited_auth(items, id, next) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// `auth` unless it defers to its parent.
fn explicit_auth(auth: Option<&AuthConfig>) -> Option<&AuthConfig> {
    auth.filter(|a| !matches!(a, AuthConfig::Inherit))
}

fn collect_planned(
    items: &[CollectionItem],
    path: &mut Vec<String>,
    inherited: Option<&AuthConfig>,
    out: &mut Vec<PlannedRequest>,
) {
    for item in items {
        match item {
            CollectionItem::Request(req) => {
                let mut request = (**req).clone();
                request.auth = effective_auth(req.auth.as_ref(), inherited);
                out.push(PlannedRequest {
                    request,
                    folder: (!path.is_empty()).then(|| path.join("/")),
                });
            }
            CollectionItem::Folder {
                name, items, auth, ..
            } => {
                let next = explicit_auth(auth.as_ref()).or(inherited);
                path.push(name.clone());
                collect_planned(items, path, next, out);
                path.pop();
            }
        }
    }
}

/// Depth-first search for a folder; on success `path` holds its full path.
fn find_folder<'a>(
    items: &'a [CollectionItem],
    selector: &str,
    path: &mut Vec<String>,
    inherited: Option<&'a AuthConfig>,
) -> Option<(&'a [CollectionItem], Option<&'a AuthConfig>)> {
    for item in items {
        let CollectionItem::Folder {
            id,
            name,
            items: children,
            auth,
            ..
        } = item
        else {
            continue;
        };
        let auth = explicit_auth(auth.as_ref()).or(inherited);
        path.push(name.clone());
        let full_path = path.join("/");
        if name.eq_ignore_ascii_case(selector)
            || full_path.eq_ignore_ascii_case(selector)
            || id.to_string() == selector
        {
            return Some((children, auth));
        }
        if let Some(found) = find_folder(children, selector, path, auth) {
            return Some(found);
        }
        path.pop();
    }
    None
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestRunStatus {
    /// Response received and every assertion passed.
    Passed,
    /// Response received but at least one assertion failed.
    Failed,
    /// The request could not be sent (network error, bad URL...).
    Error,
    /// Not run (unsupported protocol, or never reached after a bail).
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssertionOutcome {
    pub description: String,
    pub passed: bool,
    pub message: String,
    pub actual: Option<String>,
}

/// Outcome of one request in a run.
#[derive(Debug, Clone, Serialize)]
pub struct RequestRunResult {
    /// Position in the run (across iterations).
    pub index: usize,
    /// 1-based iteration number.
    pub iteration: u32,
    pub request_id: Uuid,
    pub name: String,
    pub folder: Option<String>,
    pub method: String,
    /// URL after variable resolution.
    pub url: String,
    pub status: RequestRunStatus,
    pub http_status: Option<u16>,
    pub duration_ms: u64,
    pub assertions: Vec<AssertionOutcome>,
    pub error: Option<String>,
    /// The response (with `assertion_results` filled). Not part of reports.
    #[serde(skip)]
    pub response: Option<Response>,
}

impl RequestRunResult {
    pub fn assertions_passed(&self) -> usize {
        self.assertions.iter().filter(|a| a.passed).count()
    }

    pub fn assertions_failed(&self) -> usize {
        self.assertions.len() - self.assertions_passed()
    }

    /// Display label: `folder/name` or just `name`.
    pub fn display_name(&self) -> String {
        match &self.folder {
            Some(folder) => format!("{folder}/{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Report of a whole run.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub collection: String,
    pub folder: Option<String>,
    pub environment: Option<String>,
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
    pub skipped: usize,
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    /// True if the run stopped early because of `stop_on_failure`.
    pub bailed: bool,
    /// True if the run was cancelled before finishing.
    pub cancelled: bool,
    pub results: Vec<RequestRunResult>,
}

impl RunSummary {
    /// Build a summary (counters included) from per-request results.
    pub fn from_results(
        collection: impl Into<String>,
        folder: Option<String>,
        environment: Option<String>,
        started_at: DateTime<Utc>,
        duration: Duration,
        results: Vec<RequestRunResult>,
    ) -> Self {
        let count = |s: RequestRunStatus| results.iter().filter(|r| r.status == s).count();
        Self {
            collection: collection.into(),
            folder,
            environment,
            started_at,
            duration_ms: duration.as_millis() as u64,
            total: results.len(),
            passed: count(RequestRunStatus::Passed),
            failed: count(RequestRunStatus::Failed),
            errors: count(RequestRunStatus::Error),
            skipped: count(RequestRunStatus::Skipped),
            assertions_passed: results
                .iter()
                .map(RequestRunResult::assertions_passed)
                .sum(),
            assertions_failed: results
                .iter()
                .map(RequestRunResult::assertions_failed)
                .sum(),
            bailed: false,
            cancelled: false,
            results,
        }
    }

    /// No failed or errored request.
    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.errors == 0
    }
}

/// Progress reported to the `on_event` callback of [`run_collection`].
#[derive(Debug, Clone)]
pub enum RunEvent {
    /// The run is about to start `total` requests.
    Started { total: usize },
    /// Request `index` is being sent.
    RequestStarted { index: usize, name: String },
    /// Request `index` finished (or was skipped).
    RequestFinished(Box<RequestRunResult>),
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Skip reason of the requests left unrun when `stop_on_failure` bails.
pub const NOT_RUN_BAIL: &str = "not run (bail)";

/// Run the requests of `collection` (or `options.folder`) sequentially.
///
/// Fails only if `options.iterations` is 0 or the folder doesn't exist;
/// request failures are part of the returned summary. When the run bails
/// (`stop_on_failure`), every request that never ran is reported as skipped
/// with reason [`NOT_RUN_BAIL`].
pub async fn run_collection(
    client: &HttpClient,
    collection: &Collection,
    environment: Option<&Environment>,
    options: &RunnerOptions,
    mut on_event: impl FnMut(&RunEvent),
) -> Result<RunSummary> {
    if options.iterations == 0 {
        anyhow::bail!("iterations must be at least 1");
    }
    let plan = plan_requests(collection, options.folder.as_deref())?;
    let iterations = options.iterations;
    let total = plan.len() * iterations as usize;
    let resolver =
        VariableResolver::from_context(None, &collection.variables, environment, None, None);

    let started_at = Utc::now();
    let start = Instant::now();
    let mut results = Vec::with_capacity(total);
    let mut bailed = false;
    on_event(&RunEvent::Started { total });

    'run: for iteration in 1..=iterations {
        for planned in &plan {
            let index = results.len();
            if index > 0 && options.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(options.delay_ms)).await;
            }
            on_event(&RunEvent::RequestStarted {
                index,
                name: planned.request.name.clone(),
            });
            let result = run_one(client, planned, &resolver, index, iteration).await;
            let stop = options.stop_on_failure
                && matches!(
                    result.status,
                    RequestRunStatus::Failed | RequestRunStatus::Error
                );
            on_event(&RunEvent::RequestFinished(Box::new(result.clone())));
            results.push(result);
            if stop {
                bailed = results.len() < total;
                break 'run;
            }
        }
    }

    if bailed {
        // Report the rest of the plan (this and later iterations) as skipped.
        let ran = results.len();
        let remaining = (1..=iterations)
            .flat_map(|iteration| plan.iter().map(move |planned| (iteration, planned)))
            .skip(ran);
        for (iteration, planned) in remaining {
            let mut result = pending_result(planned, &resolver, results.len(), iteration);
            result.error = Some(NOT_RUN_BAIL.to_string());
            on_event(&RunEvent::RequestFinished(Box::new(result.clone())));
            results.push(result);
        }
    }

    let folder = options
        .folder
        .as_deref()
        .and_then(|f| resolve_folder_path(collection, f));
    let mut summary = RunSummary::from_results(
        collection.name.clone(),
        folder,
        environment.map(|e| e.name.clone()),
        started_at,
        start.elapsed(),
        results,
    );
    summary.bailed = bailed;
    Ok(summary)
}

/// A not-yet-run (skipped) result for `planned`. Its `url` is the resolved
/// URL with the enabled query params appended, as actually sent.
fn pending_result(
    planned: &PlannedRequest,
    resolver: &VariableResolver,
    index: usize,
    iteration: u32,
) -> RequestRunResult {
    let request = &planned.request;
    let url = append_query_params(
        &resolver.resolve(&request.url),
        &resolver.resolve_params(&request.params),
        |s| url::form_urlencoded::byte_serialize(s.as_bytes()).collect(),
    );
    RequestRunResult {
        index,
        iteration,
        request_id: request.id,
        name: request.name.clone(),
        folder: planned.folder.clone(),
        method: request.method.as_str().to_string(),
        url,
        status: RequestRunStatus::Skipped,
        http_status: None,
        duration_ms: 0,
        assertions: Vec::new(),
        error: None,
        response: None,
    }
}

async fn run_one(
    client: &HttpClient,
    planned: &PlannedRequest,
    resolver: &VariableResolver,
    index: usize,
    iteration: u32,
) -> RequestRunResult {
    let request = &planned.request;
    let mut result = pending_result(planned, resolver, index, iteration);

    match &request.protocol {
        Protocol::Http => {}
        other => {
            let kind = match other {
                Protocol::WebSocket => "WebSocket",
                Protocol::Sse => "SSE",
                _ => "gRPC",
            };
            result.method = kind.to_string();
            result.error = Some(format!(
                "{kind} requests are not run by the collection runner"
            ));
            return result;
        }
    }

    if request.url.trim().is_empty() {
        result.status = RequestRunStatus::Error;
        result.error = Some("Request has no URL".into());
        return result;
    }

    let start = Instant::now();
    match client.send(request, resolver).await {
        Ok(mut response) => {
            let assertion_results = AssertionEngine::run_assertions(&request.assertions, &response);
            result.assertions = assertion_results
                .iter()
                .map(|r| AssertionOutcome {
                    description: r.assertion.kind.description(),
                    passed: r.passed,
                    message: r.message.clone(),
                    actual: r.actual_value.clone(),
                })
                .collect();
            response.assertion_results = assertion_results;
            result.status = if result.assertions.iter().all(|a| a.passed) {
                RequestRunStatus::Passed
            } else {
                RequestRunStatus::Failed
            };
            result.http_status = Some(response.status);
            result.duration_ms = response.timing.total.as_millis() as u64;
            result.response = Some(response);
        }
        Err(e) => {
            result.status = RequestRunStatus::Error;
            result.duration_ms = start.elapsed().as_millis() as u64;
            result.error = Some(format!("{e:#}"));
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Report writers
// ---------------------------------------------------------------------------

/// Pretty-printed JSON report.
pub fn to_json(summary: &RunSummary) -> serde_json::Result<String> {
    serde_json::to_string_pretty(summary)
}

/// JUnit XML report (one `<testcase>` per request) for CI systems.
pub fn to_junit_xml(summary: &RunSummary) -> String {
    let suite_name = match &summary.folder {
        Some(folder) => format!("{}/{folder}", summary.collection),
        None => summary.collection.clone(),
    };
    let secs = |ms: u64| format!("{:.3}", ms as f64 / 1000.0);

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        xml,
        "<testsuites name=\"hitt\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{}\">",
        summary.total,
        summary.failed,
        summary.errors,
        summary.skipped,
        secs(summary.duration_ms)
    );
    let _ = writeln!(
        xml,
        "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{}\" timestamp=\"{}\">",
        xml_escape(&suite_name),
        summary.total,
        summary.failed,
        summary.errors,
        summary.skipped,
        secs(summary.duration_ms),
        summary.started_at.format("%Y-%m-%dT%H:%M:%S")
    );
    for r in &summary.results {
        let classname = match &r.folder {
            Some(folder) => format!("{}.{}", summary.collection, folder.replace('/', ".")),
            None => summary.collection.clone(),
        };
        let mut name = format!("{} {}", r.method, r.name);
        if r.iteration > 1 {
            let _ = write!(name, " (iteration {})", r.iteration);
        }
        let _ = write!(
            xml,
            "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\"",
            xml_escape(&name),
            xml_escape(&classname),
            secs(r.duration_ms)
        );
        match r.status {
            RequestRunStatus::Passed => xml.push_str("/>\n"),
            RequestRunStatus::Failed => {
                let failed: Vec<&AssertionOutcome> =
                    r.assertions.iter().filter(|a| !a.passed).collect();
                let details: Vec<String> = failed
                    .iter()
                    .map(|a| format!("{}: {}", a.description, a.message))
                    .collect();
                let _ = write!(
                    xml,
                    ">\n      <failure message=\"{} of {} assertion(s) failed\" type=\"AssertionFailure\">{}</failure>\n    </testcase>\n",
                    failed.len(),
                    r.assertions.len(),
                    xml_escape(&details.join("\n"))
                );
            }
            RequestRunStatus::Error => {
                let msg = r.error.as_deref().unwrap_or("request failed");
                let _ = write!(
                    xml,
                    ">\n      <error message=\"{}\" type=\"RequestError\"/>\n    </testcase>\n",
                    xml_escape(msg)
                );
            }
            RequestRunStatus::Skipped => {
                let msg = r.error.as_deref().unwrap_or("skipped");
                let _ = write!(
                    xml,
                    ">\n      <skipped message=\"{}\"/>\n    </testcase>\n",
                    xml_escape(msg)
                );
            }
        }
    }
    xml.push_str("  </testsuite>\n</testsuites>\n");
    xml
}

/// Human-readable report, one line per request plus totals.
pub fn to_text(summary: &RunSummary) -> String {
    let mut out = String::new();
    let _ = write!(out, "Collection: {}", summary.collection);
    if let Some(folder) = &summary.folder {
        let _ = write!(out, "  folder: {folder}");
    }
    if let Some(env) = &summary.environment {
        let _ = write!(out, "  env: {env}");
    }
    out.push_str("\n\n");
    for r in &summary.results {
        out.push_str(&result_line(r));
        out.push('\n');
        for a in r.assertions.iter().filter(|a| !a.passed) {
            let _ = writeln!(out, "      ✗ {}: {}", a.description, a.message);
        }
    }
    let _ = writeln!(
        out,
        "\nRequests:   {} total, {} passed, {} failed, {} errors, {} skipped",
        summary.total, summary.passed, summary.failed, summary.errors, summary.skipped
    );
    let _ = writeln!(
        out,
        "Assertions: {} passed, {} failed",
        summary.assertions_passed, summary.assertions_failed
    );
    let _ = write!(
        out,
        "Duration:   {:.2}s",
        summary.duration_ms as f64 / 1000.0
    );
    if summary.bailed {
        out.push_str("  (stopped on first failure)");
    }
    if summary.cancelled {
        out.push_str("  (cancelled)");
    }
    out.push('\n');
    out
}

/// One report line: `✓ GET  folder/name  200  12ms  2/2`.
pub fn result_line(r: &RequestRunResult) -> String {
    let icon = status_icon(r.status);
    let name = r.display_name();
    match r.status {
        RequestRunStatus::Passed | RequestRunStatus::Failed => format!(
            "  {icon} {:<7} {name}  {}  {}ms  {}/{} assertions",
            r.method,
            r.http_status.map_or_else(|| "-".into(), |s| s.to_string()),
            r.duration_ms,
            r.assertions_passed(),
            r.assertions.len()
        ),
        RequestRunStatus::Error => format!(
            "  {icon} {:<7} {name}  error: {}",
            r.method,
            r.error.as_deref().unwrap_or("request failed")
        ),
        RequestRunStatus::Skipped => format!(
            "  {icon} {:<7} {name}  skipped: {}",
            r.method,
            r.error.as_deref().unwrap_or("")
        ),
    }
}

pub fn status_icon(status: RequestRunStatus) -> &'static str {
    match status {
        RequestRunStatus::Passed => "✓",
        RequestRunStatus::Failed | RequestRunStatus::Error => "✗",
        RequestRunStatus::Skipped => "-",
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters other than tab/newline are invalid in XML 1.0.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}
