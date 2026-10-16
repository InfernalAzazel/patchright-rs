//! Rust counterpart of patchright-python's `patch_python_package.py`.
//!
//! Patches a playwright-rs checkout in `playwright-rust/` in place, turning
//! its `playwright-rs` crate into `patchright-rs`, driven by `patchright-core`.
//!
//! ```sh
//! git clone --depth 1 --branch v0.20.0 https://github.com/padamson/playwright-rust
//! patchright_core_version=1.64.1 patchright_version=0.20.0 cargo run
//! ```

mod utils;

use std::env;
use std::fs;
use std::path::Path;
use utils::{
    ROOT, insert_after, insert_before, patch, patch_all, read, replace_word, rust_files, write,
};

const CRATE: &str = "crates/playwright";

fn main() {
    let patchright_core_version =
        env::var("patchright_core_version").expect("patchright_core_version must be set");
    let patchright_version = env::var("patchright_version").expect("patchright_version must be set");

    // Patching Cargo.toml
    let manifest = &format!("{CRATE}/Cargo.toml");
    patch(
        manifest,
        "name = \"playwright-rs\"\npath = \"src/bin/playwright_rs.rs\"",
        "name = \"patchright-rs\"\npath = \"src/bin/patchright_rs.rs\"",
    );
    patch(manifest, "name = \"playwright-rs\"", "name = \"patchright-rs\"");
    patch(manifest, "\nversion.workspace = true", &format!("\nversion = \"{patchright_version}\""));
    patch(
        manifest,
        "repository.workspace = true",
        "repository = \"https://github.com/InfernalAzazel/patchright-rs\"",
    );
    patch(
        manifest,
        "description = \"Rust bindings for Microsoft Playwright\"",
        "description = \"Undetected Rust version of the Playwright testing and automation library.\"",
    );
    patch(
        manifest,
        "keywords = [\"playwright\", \"browser\", \"automation\", \"testing\", \"e2e\"]",
        "keywords = [\"patchright\", \"playwright\", \"undetected\", \"automation\", \"browser\"]",
    );
    insert_after(manifest, "[lib]\n", "name = \"patchright_rs\"\n");
    // xtask depends on `playwright-rs` by path and would no longer resolve.
    patch("Cargo.toml", "    \"crates/xtask\",\n", "");

    // Patching the driver (build_driver.py / DRIVER_VERSION)
    let build_rs = &format!("{CRATE}/build.rs");
    let driver_urls = &format!("{CRATE}/src/build_support/driver_urls.rs");
    let driver = &format!("{CRATE}/src/server/driver.rs");
    let source = read(build_rs);
    let version_line = source
        .lines()
        .find(|l| l.starts_with("const PLAYWRIGHT_VERSION: &str = "))
        .expect("PLAYWRIGHT_VERSION not found in build.rs");
    patch(
        build_rs,
        version_line,
        &format!("const PLAYWRIGHT_VERSION: &str = \"{patchright_core_version}\";"),
    );
    patch_all(driver_urls, "/playwright-core/-/playwright-core-", "/patchright-core/-/patchright-core-");
    for file in [build_rs, driver_urls, driver] {
        patch_all(file, "playwright-rust", "patchright-rust");
    }
    // v0.20.0 moved the driver directory name and the npm package name into
    // `driver_urls`; `build.rs` and `driver.rs` no longer spell them out.
    patch_all(driver_urls, "playwright-{", "patchright-{");
    patch_all(driver_urls, "playwright-1.", "patchright-1.");
    patch(
        driver,
        "        node_modules.join(\"playwright\"),\n        node_modules.join(\"@playwright\").join(\"test\"),",
        "        node_modules.join(\"patchright-core\"),\n        node_modules.join(\"patchright\"),",
    );

    // Patching imports: playwright_rs -> patchright_rs
    fs::rename(
        Path::new(ROOT).join(CRATE).join("src/bin/playwright_rs.rs"),
        Path::new(ROOT).join(CRATE).join("src/bin/patchright_rs.rs"),
    )
    .unwrap();
    let mut files = Vec::new();
    rust_files(&Path::new(ROOT).join(CRATE), &mut files);
    for file in files {
        let source = read(&file);
        let renamed = replace_word(&source, "playwright_rs", "patchright_rs");
        let renamed = replace_word(&renamed, "playwright-rs", "patchright-rs");
        if renamed != source {
            write(&file, &renamed);
        }
    }

    patch_isolated_context();
    patch_focus_control();
    patch_route();
    patch_inject_route();
    patch_dialog();
    patch_no_console_window();

    // Stealth tests against the detection sites listed in the README
    write(
        &format!("{CRATE}/tests/patchright_stealth.rs"),
        &fs::read_to_string("src/tests/patchright_stealth.rs").unwrap(),
    );

    // Write the project's README to the README used in the release
    write("README.md", &fs::read_to_string("README.md").unwrap());

    // The runtime driver download. Last, so every anchor below sees the tree
    // the earlier patches have already settled.
    patch_driver_download();
    patch_driver_assembly_in_lib();
    patch_driver_download_module();
    patch_runtime_download();
    patch_launch_uses_ensure_driver();
    patch_changelog();
}

/// `evaluate` / `evaluate_handle` / `evaluate_all` run in an isolated world
/// by default. Rust has no keyword arguments, so instead of an
/// `isolated_context=` parameter on each method, the flag is added once in
/// `Channel::send` and chosen per task with `main_world(...)`.
fn patch_isolated_context() {
    let lib = &format!("{CRATE}/src/lib.rs");
    write(&format!("{CRATE}/src/patchright.rs"), PATCHRIGHT_MODULE);
    insert_after(lib, "pub mod protocol;\n", "pub mod patchright;\n");
    insert_after(
        lib,
        "pub use error::{Error, Result};\n",
        "\npub use patchright::{main_world, with_isolated_context};\n",
    );
    patch(
        &format!("{CRATE}/src/server/channel.rs"),
        "        let params_value = serde_json::to_value(params)?;\n",
        "        let mut params_value = serde_json::to_value(params)?;\n        crate::patchright::apply_isolated_context(method, &mut params_value);\n",
    );
}

const PATCHRIGHT_MODULE: &str = r#"//! Patchright extensions to the Playwright API.

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
"#;

/// `focusControl` option for `new_context`, `new_page` and
/// `launch_persistent_context`, which all take `BrowserContextOptions`.
fn patch_focus_control() {
    let file = &format!("{CRATE}/src/protocol/browser_context/options.rs");
    insert_after(
        file,
        "    #[serde(skip_serializing_if = \"Option::is_none\")]\n    pub service_workers: Option<String>,\n",
        "\n    /// Patchright: lets the driver emulate focus for the context's pages.\n    #[serde(skip_serializing_if = \"Option::is_none\")]\n    pub focus_control: Option<bool>,\n",
    );
    patch(
        file,
        "    service_workers: Option<String>,\n    record_har: Option<RecordHar>,\n",
        "    service_workers: Option<String>,\n    focus_control: Option<bool>,\n    record_har: Option<RecordHar>,\n",
    );
    insert_after(
        file,
        "    pub fn service_workers(mut self, value: String) -> Self {\n        self.service_workers = Some(value);\n        self\n    }\n",
        "\n    /// Patchright: lets the driver emulate focus for the context's pages.\n    pub fn focus_control(mut self, enabled: bool) -> Self {\n        self.focus_control = Some(enabled);\n        self\n    }\n",
    );
    insert_after(
        file,
        "            service_workers: self.service_workers,\n",
        "            focus_control: self.focus_control,\n",
    );
    // `every_key_sent_is_a_parameter_the_driver_declares` builds the options
    // struct as a literal with no `..Default::default()`, so the new field
    // breaks it until it is listed, and `focusControl` is a patchright-core
    // extension the vanilla protocol spec cannot declare.
    insert_after(
        file,
        "            service_workers: Some(String::new()),\n",
        "            focus_control: Some(false),\n",
    );
    patch(
        file,
        "        assert_eq!(undeclared, [\"storageStatePath\", \"timeout\"]);\n",
        "        assert_eq!(undeclared, [\"focusControl\", \"storageStatePath\", \"timeout\"]);\n",
    );
}

/// `patchrightInitScript` continue override, and fallback overrides that
/// accumulate like patchright-python's `_fallback_overrides`.
///
/// playwright-rs sends `continue` on every `fallback()`, so a user handler
/// and the init-script route falling back on one request would continue it
/// twice. `fallback()` now only records its overrides; the page or context
/// dispatcher continues the route once, after its last handler.
fn patch_route() {
    let route = &format!("{CRATE}/src/protocol/route.rs");
    let params = &format!("{CRATE}/src/protocol/route_params.rs");

    insert_after(
        route,
        "    /// Modified request URL (must have same protocol)\n    pub url: Option<String>,\n",
        "    /// Patchright: inject init scripts into the document this request loads.\n    pub patchright_init_script: Option<bool>,\n",
    );
    patch(
        route,
        "    post_data_bytes: Option<Vec<u8>>,\n    url: Option<String>,\n}\n",
        "    post_data_bytes: Option<Vec<u8>>,\n    url: Option<String>,\n    patchright_init_script: Option<bool>,\n}\n",
    );
    insert_before(
        route,
        "    /// Builds the ContinueOptions\n",
        "    /// Patchright: inject init scripts into the document this request loads.\n    pub fn patchright_init_script(mut self, enabled: bool) -> Self {\n        self.patchright_init_script = Some(enabled);\n        self\n    }\n\n",
    );
    patch(
        route,
        "            url: self.url,\n        }\n    }\n}\n",
        "            url: self.url,\n            patchright_init_script: self.patchright_init_script,\n        }\n    }\n}\n",
    );
    insert_after(
        params,
        "    if let Some(url) = opts.url {\n        params[\"url\"] = json!(url);\n    }\n",
        "    if opts.patchright_init_script == Some(true) {\n        params[\"patchrightInitScript\"] = json!(true);\n    }\n",
    );

    insert_after(
        route,
        "    api_request_context: Arc<Mutex<Option<APIRequestContext>>>,\n",
        "    /// Patchright: `continue` params recorded by `fallback()`.\n    fallback_overrides: Arc<Mutex<serde_json::Map<String, Value>>>,\n",
    );
    insert_after(
        route,
        "            api_request_context: Arc::new(Mutex::new(None)),\n",
        "            fallback_overrides: Arc::new(Mutex::new(serde_json::Map::new())),\n",
    );
    patch(
        route,
        "        self.continue_internal(overrides, true).await\n    }\n",
        r#"        self.record_overrides(overrides);
        Ok(())
    }

    fn record_overrides(&self, overrides: Option<ContinueOptions>) {
        let params = super::route_params::continue_params(overrides, false);
        let mut recorded = self.fallback_overrides.lock().unwrap();
        for (key, value) in params.as_object().into_iter().flatten() {
            if key != "isFallback" {
                recorded.insert(key.clone(), value.clone());
            }
        }
    }

    /// Continues a route every handler fell back on, handing it to the next
    /// routing level with the recorded overrides.
    pub(crate) async fn finish_fallback(&self) -> Result<()> {
        self.continue_internal(None, true).await
    }
"#,
    );
    patch(
        route,
        "        let params = super::route_params::continue_params(overrides, is_fallback);\n",
        "        self.record_overrides(overrides);\n        let mut params = std::mem::take(&mut *self.fallback_overrides.lock().unwrap());\n        params.insert(\"isFallback\".to_string(), Value::Bool(is_fallback));\n        let params = Value::Object(params);\n",
    );

    let dispatch_tail = "                if !route.was_handled() {\n                    continue;\n                }\n                break;\n            }\n        }\n";
    let finish = "\n        if !route.was_handled()\n            && let Err(e) = route.finish_fallback().await\n        {\n            tracing::warn!(\"Route fallback failed: {}\", e);\n        }\n";
    insert_after(&format!("{CRATE}/src/protocol/page/network.rs"), dispatch_tail, finish);
    insert_after(&format!("{CRATE}/src/protocol/browser_context/network.rs"), dispatch_tail, finish);
}

/// `install_inject_route`: before init scripts, bindings, clock or tracing
/// are used, install a catch-all route that marks documents with
/// `patchrightInitScript`. It is registered first, so it runs after every
/// user handler has fallen back.
fn patch_inject_route() {
    let page_mod = &format!("{CRATE}/src/protocol/page/mod.rs");
    let page_lifecycle = &format!("{CRATE}/src/protocol/page/lifecycle.rs");
    let page_network = &format!("{CRATE}/src/protocol/page/network.rs");
    let page_bindings = &format!("{CRATE}/src/protocol/page/bindings.rs");
    let ctx_mod = &format!("{CRATE}/src/protocol/browser_context/mod.rs");
    let ctx_lifecycle = &format!("{CRATE}/src/protocol/browser_context/lifecycle.rs");
    let ctx_network = &format!("{CRATE}/src/protocol/browser_context/network.rs");
    let ctx_bindings = &format!("{CRATE}/src/protocol/browser_context/bindings.rs");
    let clock = &format!("{CRATE}/src/protocol/clock.rs");
    let tracing = &format!("{CRATE}/src/protocol/tracing.rs");
    let install = "        self.install_inject_route().await?;\n";

    insert_after(
        page_mod,
        "    route_handlers: Arc<Mutex<Vec<RouteHandlerEntry>>>,\n",
        "    route_injecting: Arc<AtomicBool>,\n",
    );
    insert_after(
        page_lifecycle,
        "            main_frame,\n            route_handlers,\n",
        "            route_injecting: Arc::new(AtomicBool::new(false)),\n",
    );
    insert_before(
        page_network,
        "    /// Updates network interception patterns for this page\n",
        r#"    pub(crate) async fn install_inject_route(&self) -> Result<()> {
        if self.context().is_ok_and(|c| c.route_injecting())
            || self.route_injecting.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(());
        }
        let handler = Arc::new(|route: Route| -> RouteHandlerFuture {
            Box::pin(crate::patchright::inject_route_handler(route))
        });
        let entry = RouteHandlerEntry { pattern: "**/*".to_string(), handler, in_flight: Arc::default() };
        self.route_handlers.lock().unwrap().insert(0, entry);
        self.enable_network_interception().await
    }

"#,
    );
    insert_after(page_bindings, "    pub async fn add_init_script(&self, script: &str) -> Result<()> {\n", install);
    insert_before(page_bindings, "        let callback: PageBindingCallback = Arc::new(", install);

    patch(
        ctx_mod,
        "    route_handlers: Arc<Mutex<Vec<RouteHandlerEntry>>>,\n    /// APIRequestContext",
        "    route_handlers: Arc<Mutex<Vec<RouteHandlerEntry>>>,\n    route_injecting: Arc<AtomicBool>,\n    /// APIRequestContext",
    );
    insert_after(
        ctx_lifecycle,
        "            route_handlers: Arc::new(Mutex::new(Vec::new())),\n            request_context_guid,\n",
        "            route_injecting: Arc::new(AtomicBool::new(false)),\n",
    );
    insert_before(
        ctx_network,
        "    /// Handles a route event from the protocol\n    pub(super) async fn on_route_event(",
        r#"    pub(crate) fn route_injecting(&self) -> bool {
        self.route_injecting.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) async fn install_inject_route(&self) -> Result<()> {
        if self.route_injecting.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        let handler = Arc::new(|route: Route| -> RouteHandlerFuture {
            Box::pin(crate::patchright::inject_route_handler(route))
        });
        let entry = RouteHandlerEntry { pattern: "**/*".to_string(), handler, in_flight: Arc::default() };
        self.route_handlers.lock().unwrap().insert(0, entry);
        self.enable_network_interception().await
    }

"#,
    );
    insert_after(ctx_bindings, "    pub async fn add_init_script(&self, script: &str) -> Result<()> {\n", install);
    insert_before(ctx_bindings, "        let callback: BindingCallback = Arc::new(", install);

    patch(
        clock,
        "pub struct Clock {\n    channel: Channel,\n}",
        "pub struct Clock {\n    channel: Channel,\n    context: Option<crate::protocol::BrowserContext>,\n}",
    );
    patch(
        clock,
        "        Self { channel }\n    }\n",
        "        Self { channel, context: None }\n    }\n\n    pub(crate) fn with_context(mut self, context: crate::protocol::BrowserContext) -> Self {\n        self.context = Some(context);\n        self\n    }\n",
    );
    insert_after(
        clock,
        "    pub async fn install(&self, options: impl Into<Option<ClockInstallOptions>>) -> Result<()> {\n",
        "        if let Some(context) = &self.context {\n            context.install_inject_route().await?;\n        }\n",
    );
    patch(
        ctx_lifecycle,
        "        crate::protocol::clock::Clock::new(self.channel().clone())\n",
        "        crate::protocol::clock::Clock::new(self.channel().clone()).with_context(self.clone())\n",
    );

    insert_after(
        tracing,
        "    pub async fn start(&self, options: impl Into<Option<TracingStartOptions>>) -> Result<()> {\n",
        "        if let Some(context) =\n            crate::server::connection::downcast_parent::<crate::protocol::BrowserContext>(self)\n        {\n            context.install_inject_route().await?;\n        }\n",
    );
}

/// `_on_dialog`: a dialog nobody listens for is answered (accept
/// `beforeunload`, dismiss the rest) instead of blocking the page.
fn patch_dialog() {
    insert_before(
        &format!("{CRATE}/src/protocol/page/dispatch.rs"),
        "    async fn on_dialog_event(&self, dialog: Dialog) {\n",
        "    pub(crate) fn has_dialog_listeners(&self) -> bool {\n        !self.dialog.is_idle()\n    }\n\n",
    );
    patch(
        &format!("{CRATE}/src/protocol/browser_context/dispatch.rs"),
        r#"                        // Dispatch to context-level dialog handlers first
                        ctx_dialog.dispatch(dialog.clone()).await;

                        // Then forward to the Page's dialog handlers
                        let page: Page =
                            match crate::server::connection::downcast_parent::<Page>(&dialog) {
                                Some(p) => p,
                                None => return,
                            };

                        page.trigger_dialog_event(dialog).await;
"#,
        r#"                        let mut has_listeners = !ctx_dialog.is_idle();
                        ctx_dialog.dispatch(dialog.clone()).await;

                        if let Some(page) =
                            crate::server::connection::downcast_parent::<Page>(&dialog)
                        {
                            has_listeners |= page.has_dialog_listeners();
                            page.trigger_dialog_event(dialog.clone()).await;
                        }

                        if !has_listeners {
                            let _ = if dialog.type_() == "beforeunload" {
                                dialog.accept(None).await
                            } else {
                                dialog.dismiss().await
                            };
                        }
"#,
    );
}

/// Suppress the console window Windows allocates for the Node driver.
///
/// `node.exe` is a console program. A host with no console of its own — a
/// GUI-subsystem application, which is the normal shape for a Tauri app
/// (`windows_subsystem = "windows"`) — makes Windows allocate a new console
/// for the child, so a black window appears on every browser launch. Only
/// seen there: a console host (CLI, test binary, `cargo run`) hands its own
/// console down, so neither upstream's own use nor CI ever shows it.
///
/// `CREATE_NO_WINDOW` (0x0800_0000) suppresses the allocation, exactly as it
/// does for any other child process. The `cmd` here is a
/// `tokio::process::Command`, whose `creation_flags` is `#[cfg(windows)]`.
fn patch_no_console_window() {
    insert_after(
        &format!("{CRATE}/src/server/playwright_server.rs"),
        "let mut cmd = Command::new(&node_exe);",
        r#"

        // Windows allocates a console for this console-program child when the
        // host has none (GUI-subsystem apps like Tauri); suppress it.
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW"#,
    );
}

/// The `driver-download` feature: assemble the driver at *runtime*, for a
/// machine that only ever received the built binary.
///
/// `build.rs` assembles the driver on the build machine and records that
/// machine's cache path, so a distributed binary finds nothing: the recorded
/// path names the builder, and the user cache is empty on the machine that
/// runs the binary. This feature is what puts a driver on *that* machine.
///
/// On by default. A consumer that brackets the crate (`ghost-job` does) takes
/// default features, and an opt-in knob would push the work back onto every
/// downstream binary that needs it — which is the arrangement this exists to
/// remove.
fn patch_driver_download() {
    let manifest = &format!("{CRATE}/Cargo.toml");
    // The same dependency set `cli` already declares: one downloader.
    insert_before(
        manifest,
        "cli = [\n",
        "driver-download = [\n    \"dep:ureq\",\n    \"dep:rustls\",\n    \"dep:zip\",\n    \"dep:flate2\",\n    \"dep:tar\",\n]\n",
    );
    patch(
        manifest,
        "default = [\"native-tls\", \"macros\", \"ring\", \"route-service\"]\n",
        "default = [\"native-tls\", \"macros\", \"ring\", \"route-service\", \"driver-download\"]\n",
    );
}

/// Make the one assembly implementation reachable from the library.
///
/// `driver_assembly.rs` is already `include!`d by `build.rs` and the cli
/// binary; the runtime download is its third caller, so it becomes a real
/// module of the crate and its two entry points need crate visibility.
///
/// It `include!`s `driver_urls.rs` itself, so this also pulls a second copy of
/// those pure mapping functions into `driver_assembly`. That is deliberate:
/// dropping that `include!` would mean editing `build.rs` and the cli binary
/// too, and the duplication is a handful of `match` arms with no state.
fn patch_driver_assembly_in_lib() {
    let assembly = &format!("{CRATE}/src/build_support/driver_assembly.rs");
    patch(
        assembly,
        "#[allow(dead_code)]\nfn assemble_driver(",
        "#[allow(dead_code)]\npub(crate) fn assemble_driver(",
    );
    patch(
        assembly,
        "#[allow(dead_code)]\nfn driver_complete(",
        "#[allow(dead_code)]\npub(crate) fn driver_complete(",
    );
    insert_after(
        &format!("{CRATE}/src/lib.rs"),
        "#[path = \"build_support/driver_urls.rs\"]\nmod driver_urls;\n",
        "
// The runtime driver download (`driver-download`) assembles with the same
// implementation the build script and the cli binary use; this is its third
// compile point. Gated on the feature so a consumer that turns it off does
// not carry the download stack.
//
// `allow(dead_code)` because the file `include!`s `driver_urls.rs` itself, so
// the mapping functions it does not happen to call (`driver_dir_name`,
// `cached_driver_dir`) have a second, unused copy here. Removing that
// `include!` would mean editing `build.rs` and the cli binary as well. The
// duplication carries that file's `#[cfg(test)]` module too, so its tests run
// twice under `cargo test --lib`; they are pure mapping assertions, and the
// alternative is a three-way edit of files this patch does not otherwise touch.
#[cfg(feature = \"driver-download\")]
#[allow(dead_code)]
#[path = \"build_support/driver_assembly.rs\"]
mod driver_assembly;
",
    );
}

/// The module source for the runtime download.
///
/// Kept as its own file and pulled in with `include_str!` so an editor still
/// highlights it as Rust, and so a refactor breaks one short anchor instead of
/// a hundred lines of string literal.
const DRIVER_DOWNLOAD_MODULE: &str = include_str!("driver_download_module.rs");

/// The runtime driver download's module and its declaration.
fn patch_driver_download_module() {
    write(
        &format!("{CRATE}/src/server/driver_download.rs"),
        DRIVER_DOWNLOAD_MODULE,
    );
    // `driver` sits in the same parent module, so this stays crate-internal:
    // the feature is a build-time choice, not public API.
    insert_after(
        &format!("{CRATE}/src/server/mod.rs"),
        "pub mod driver;\n",
        "#[cfg(feature = \"driver-download\")]\npub(crate) mod driver_download;\n",
    );
}

/// `ensure_driver`: the runtime download, gated so it only runs when the whole
/// resolution chain missed.
///
/// It stays out of `get_driver_executable` (which is sync, cheap, and whose
/// callers include browser installation): resolution there must not turn into a
/// 100 MB download, and a misconfigured override must surface rather than be
/// papered over by one.
fn patch_runtime_download() {
    let driver = &format!("{CRATE}/src/server/driver.rs");
    insert_after(
        driver,
        "pub fn get_driver_executable() -> Result<(PathBuf, PathBuf)> {\n    resolve_driver(|name| std::env::var(name).ok())\n}\n",
        r#"
/// Locate the driver, assembling one into this machine's user cache first if
/// there is none.
///
/// [`get_driver_executable`] covers a machine that built the crate: the build
/// script assembled a driver there and the user cache is populated. A machine
/// that only received a built binary has neither — the recorded path names the
/// builder, and nothing ever ran the assembler here. This is the entry point
/// for that case, and the only place the crate reaches the network at runtime.
///
/// Downloads only when resolution finds nothing at all. A driver override that
/// is set but wrong stays an error: it is a misconfiguration, and fetching a
/// driver would hide it.
///
/// # Errors
///
/// Whatever [`get_driver_executable`] returns, when it resolves or when the
/// download is not attempted; [`Error::DriverDownloadFailed`] when the
/// assembly was tried and failed, naming the failure and repeating the ways to
/// provide a driver.
pub async fn ensure_driver() -> Result<(PathBuf, PathBuf)> {
    match get_driver_executable() {
        Ok(found) => return Ok(found),
        Err(e) if !should_download(&e) => return Err(e),
        Err(_) => {}
    }

    #[cfg(feature = "driver-download")]
    match tokio::task::spawn_blocking(|| {
        crate::server::driver_download::assemble_into_user_cache(|name| std::env::var(name).ok())
    })
    .await
    {
        // Fetching and unpacking ~100 MB would otherwise hold a runtime worker
        // for as long as it takes.
        Ok(Ok(Some(dir))) => {
            tracing::info!(
                target: "patchright_rs::driver_download",
                "no Playwright driver on this machine; assembled one at {}",
                dir.display()
            );
        }
        Ok(Ok(None)) => {}
        Ok(Err(e)) => return Err(Error::DriverDownloadFailed(e.to_string())),
        Err(e) => {
            return Err(Error::DriverDownloadFailed(format!(
                "the driver assembly task did not finish: {e}"
            )));
        }
    }

    get_driver_executable()
}

/// Whether a resolution failure should trigger the runtime download.
///
/// Only "nothing was found" does. A misconfiguration names the variable that is
/// wrong and is the user's to fix; downloading past it would bury that.
fn should_download(error: &Error) -> bool {
    matches!(error, Error::ServerNotFound)
}

/// The text of [`Error::DriverDownloadFailed`]: what failed, then the same ways
/// out [`Error::ServerNotFound`] lists.
pub(crate) fn download_failed_message(reason: &str) -> String {
    format!(
        "Failed to download the Playwright driver: {reason}\n\n\
        No driver was found on this machine and assembling one from the \
        patchright-core and nodejs.org artifacts failed — no network access, a \
        proxy, or a blocked host.\n\n{}",
        BundledDriver::for_this_build().server_not_found_message()
    )
}
"#,
    );

    // The error variant lives here, not with the feature: its `#[error]` calls
    // `download_failed_message`, so the two have to land together or the crate
    // does not compile. `Error` is `#[non_exhaustive]`, so adding a variant
    // breaks no downstream `match`.
    insert_after(
        &format!("{CRATE}/src/error.rs"),
        "    ServerNotFound,\n",
        r#"
    /// The runtime driver download failed.
    ///
    /// The crate tried to assemble a driver on this machine (see the
    /// `driver-download` feature) and could not reach the artifacts. The
    /// message names the failure and repeats the ways to provide a driver.
    #[error("{}", crate::server::driver::download_failed_message(.0))]
    DriverDownloadFailed(String),
"#,
    );

    // The three-way decision is a pure function, so it gets a pure test; the
    // async wrapper around it is straight-line code over tested parts.
    insert_before(
        driver,
        "    fn install_args_request_system_deps_only_when_asked() {",
        r#"    #[test]
    fn only_a_missing_driver_triggers_the_runtime_download() {
        assert!(should_download(&Error::ServerNotFound));
        assert!(
            !should_download(&Error::DriverMisconfigured("x".into())),
            "a bad override must surface, not be papered over by a download"
        );
        assert!(!should_download(&Error::LaunchFailed("x".into())));
    }

    #[test]
    fn the_download_failure_names_the_reason_and_the_ways_out() {
        let message = download_failed_message("download of https://example.invalid failed");
        assert!(message.starts_with("Failed to download the Playwright driver:"));
        assert!(message.contains("https://example.invalid"));
        for token in ["patchright-rs install", "PLAYWRIGHT_DRIVER_PATH"] {
            assert!(message.contains(token), "must name {token}:\n{message}");
        }
    }

"#,
    );
}

/// Point the launch path at `ensure_driver`, which is what the function's own
/// doc comment has claimed since before the download existed.
fn patch_launch_uses_ensure_driver() {
    let server = &format!("{CRATE}/src/server/playwright_server.rs");
    // `get_driver_executable` is used here and nowhere else in this file, so
    // this replaces the import: extending it would leave an unused one.
    patch(
        server,
        "use crate::server::driver::get_driver_executable;",
        "use crate::server::driver::ensure_driver;",
    );
    patch(
        server,
        "        // Get the driver executable paths\n        // The driver should already be downloaded by build.rs\n        let (node_exe, cli_js) = get_driver_executable()?;",
        "        // Resolve the driver, assembling one into this machine's user cache\n        // if there is none: build.rs only ever ran on the build machine.\n        let (node_exe, cli_js) = ensure_driver().await?;",
    );
}

/// Record the behaviour change where a consumer looks for it.
fn patch_changelog() {
    insert_after(
        &format!("{CRATE}/CHANGELOG.md"),
        "## [Unreleased]\n",
        r#"
### Added

- **`ensure_driver`: the driver is assembled on the machine that runs it, when that machine has none.** The build script assembles the driver at compile time and records the *builder's* cache path, which is the wrong machine for anyone who receives a built binary: the recorded path names the builder, the user cache is empty there, and every launch fails with `Error::ServerNotFound`. `PlaywrightServer::launch` now calls `ensure_driver`, which resolves exactly as before and, only when nothing is found at all, assembles the driver into this machine's user cache — the same directory the resolution chain already probes, so no environment variable has to be set. Behind the new `driver-download` feature, on by default because the consumers this exists for bracket the crate with default features. A `PLAYWRIGHT_DRIVER_PATH` that is set but wrong still fails as `Error::DriverMisconfigured` rather than being papered over, and `PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD` turns the runtime download off for a host that must not reach the network. A download that was attempted and failed is its own error, `Error::DriverDownloadFailed`, whose message names the failure and repeats the ways to provide a driver.
- **`PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD` now applies at runtime too.** It was a build-time knob; a process that has it set assembles nothing either, which is what a compile-only job or an offline host needs. A build machine and the machine that runs the binary are different processes, so a build that skipped the download can still ship a binary that fetches a driver on first launch.

### Changed

- **A machine with no driver now reaches the network on first launch.** Up to ~100 MB (a Node runtime and the `patchright-core` tarball) is fetched once per machine and cached under the user cache directory. The download runs off a tokio worker (`spawn_blocking`), and an assembly that fails leaves the launch failing as it did before, with the reason attached.
"#,
    );
}
