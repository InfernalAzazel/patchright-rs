//! Patchright extensions to the Playwright API.

use std::future::Future;

tokio::task_local! {
    static ISOLATED_CONTEXT: bool;
}

/// Protocol methods whose `isolatedContext` parameter patchright-core honors.
const EVALUATE_METHODS: &[&str] = &[
    "evaluateExpression",
    "evaluateExpressionHandle",
    "evalOnSelectorAll",
];

/// Runs `future` with evaluation in the isolated world (`true`, the default)
/// or the page's main world (`false`). Equivalent to patchright-python's
/// `isolated_context=` argument.
pub async fn with_isolated_context<F: Future>(isolated: bool, future: F) -> F::Output {
    ISOLATED_CONTEXT.scope(isolated, future).await
}

/// Runs `future` with evaluation in the page's main world.
pub async fn main_world<F: Future>(future: F) -> F::Output {
    with_isolated_context(false, future).await
}

pub(crate) fn apply_isolated_context(method: &str, params: &mut serde_json::Value) {
    if EVALUATE_METHODS.contains(&method)
        && let Some(object) = params.as_object_mut()
    {
        let isolated = ISOLATED_CONTEXT.try_with(|v| *v).unwrap_or(true);
        object.entry("isolatedContext").or_insert(isolated.into());
    }
}

/// Route installed by `install_inject_route`: lets the driver inject init
/// scripts and bindings into http(s) documents.
pub(crate) async fn inject_route_handler(route: crate::protocol::Route) -> crate::Result<()> {
    let request = route.request();
    let options = (request.resource_type() == "document" && request.url().starts_with("http"))
        .then(|| crate::protocol::ContinueOptions::builder().patchright_init_script(true).build());
    route.fallback(options).await
}
