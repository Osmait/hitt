use crate::core::history::HistoryEntry;
use crate::core::request::Request;
use crate::core::variables::VariableResolver;
use crate::testing::assertion_engine::AssertionEngine;
use crate::{Core, CoreEvent};

pub(crate) async fn send(core: &mut Core, request: Request) -> Vec<CoreEvent> {
    let environment = core
        .active_environment
        .as_ref()
        .and_then(|name| core.environments.iter().find(|e| &e.name == name));

    let resolver = VariableResolver::from_context(None, &[], environment, None, None);

    match core.client.send(&request, &resolver).await {
        Ok(mut response) => {
            if !request.assertions.is_empty() {
                response.assertion_results =
                    AssertionEngine::run_assertions(&request.assertions, &response);
            }

            // Record in history.
            let mut entry = HistoryEntry::new(request.method, resolver.resolve(&request.url));
            entry.status = Some(response.status);
            entry.duration_ms = Some(response.timing.total.as_millis() as u64);
            entry.size_bytes = Some(response.size.total());
            entry.request_id = Some(request.id);
            entry.response_body = response.body_text().map(str::to_string);
            entry.response_headers.clone_from(&response.headers);
            entry.request = Some(request.clone());
            core.history.add(entry);

            vec![CoreEvent::RequestCompleted(Box::new(response))]
        }
        Err(e) => vec![CoreEvent::Error(e.to_string())],
    }
}
