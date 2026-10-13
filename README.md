<h1 align="center">
    🎭 Patchright Rust
</h1>


<p align="center">
    <a href="#copyright-and-license">
        <img src="https://img.shields.io/badge/License-Apache%202.0-green">
    </a>
    <a>
        <img src="https://img.shields.io/badge/Based%20on-Playwright-goldenrod">
    </a>
    <a>
        <img src="https://img.shields.io/badge/Driver-Patched-blue">
    </a>
    <a href="https://github.com/InfernalAzazel/patchright-rs/actions/workflows/patchright_release.yml">
        <img src="https://github.com/InfernalAzazel/patchright-rs/actions/workflows/patchright_release.yml/badge.svg">
    </a>
    <br/>
    <a href="https://github.com/InfernalAzazel/patchright-rs/tags">
        <img alt="Patchright Version" src="https://img.shields.io/github/v/tag/InfernalAzazel/patchright-rs?label=Version">
    </a>
    <a href="https://github.com/InfernalAzazel/patchright-rs">
        <img src="https://img.shields.io/badge/Package-Rust-orange">
    </a>
</p>

#### Patchright is a patched and undetected version of the Playwright Testing and Automation Framework. </br> It can be used as a drop-in replacement for Playwright.

> [!NOTE]  
> This repository serves the Patchright-Rust Package, built by patching [playwright-rs](https://github.com/padamson/playwright-rust) the way [Patchright-Python](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright-python) patches playwright-python.
> To use Patchright with NodeJS, check out the [NodeJS Package](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright-nodejs).
> Also check out the main [Patchright Driver Repository](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright)

---

## Install it from GitHub

Patchright-Rust is not on crates.io. Every release is a `v<version>` [tag](https://github.com/InfernalAzazel/patchright-rs/tags) of this repository that contains the patched crate.

```toml
# Cargo.toml
[dependencies]
patchright-rs = { git = "https://github.com/InfernalAzazel/patchright-rs", tag = "v0.19.0" }
tokio = { version = "1", features = ["full"] }
```

```bash
# Install Chromium-Driver for Patchright
cargo install --git https://github.com/InfernalAzazel/patchright-rs --tag v0.19.0 patchright-rs --features cli
patchright-rs install chromium
```

---

## Usage
#### Just change the dependency and use it like playwright-rs. Patchright is a drop-in-replacement for Playwright!

> [!IMPORTANT]  
> Patchright only patches CHROMIUM based browsers. Firefox and Webkit are not supported.

```rust
// patchright here!
use patchright_rs::Playwright;

#[tokio::main]
async fn main() -> patchright_rs::Result<()> {
    let playwright = Playwright::launch().await?;
    let browser = playwright.chromium().launch().await?;
    let page = browser.new_page().await?;
    page.goto("http://playwright.dev", None).await?;
    std::fs::write("example-chromium.png", page.screenshot(None).await?)?;
    browser.close().await?;
    Ok(())
}
```

### Best Practice  - use Chrome without Fingerprint Injection

To be completely undetected, use the following configuration:
```rust
use patchright_rs::protocol::BrowserContextOptions;

let context = playwright
    .chromium()
    .launch_persistent_context_with_options(
        "...", // user_data_dir
        BrowserContextOptions::builder()
            .channel("chrome".to_string())
            .headless(false)
            .no_viewport(true)
            // do NOT add custom browser headers or user_agent
            .build(),
    )
    .await?;
```

> [!NOTE]  
> We recommend using Google Chrome instead of Chromium.
> You can install it via `patchright-rs install chrome` (or via any other installation method) and use it with `.channel("chrome")`.

---

## Patches

The patches below live in the [`patchright-core`](https://www.npmjs.com/package/patchright-core) driver, which this package uses in place of `playwright-core`.

### [Runtime.enable](https://vanilla.aslushnikov.com/?Runtime.enable) Leak
This is the biggest Patch Patchright uses. To avoid detection by this leak, patchright avoids using [Runtime.enable](https://vanilla.aslushnikov.com/?Runtime.enable) by executing Javascript in (isolated) ExecutionContexts.

### [Console.enable](https://vanilla.aslushnikov.com/?Console.enable) Leak
Patchright patches this leak by disabling the Console API all together. This means, console functionality will not work in Patchright. If you really need the console, you might be better off using Javascript loggers, although they also can be easily detected.

### Command Flags Leaks
Patchright tweaks the Playwright Default Args to avoid detection by Command Flag Leaks. This (most importantly) affects:
- `--disable-blink-features=AutomationControlled` (added) to avoid navigator.webdriver detection.
- `--enable-automation` (removed) to avoid navigator.webdriver detection.
- `--disable-popup-blocking` (removed) to avoid popup crashing.
- `--disable-component-update` (removed) to avoid detection as a Stealth Driver.
- `--disable-default-apps` (removed) to enable default apps.
- `--disable-extensions` (removed) to enable extensions

### General Leaks
Patchright patches some general leaks in the Playwright codebase. This mainly includes poor setups and obvious detection points.

### Closed Shadow Roots
Patchright is able to interact with elements in Closed Shadow Roots. Just use normal locators and Patchright will do the rest.
<br/>
Patchright is now also able to use XPaths in Closed Shadow Roots.

---

## Stealth

With the right setup, Patchright currently is considered undetectable.
Patchright passes:
- [Brotector](https://github.com/kaliiiiiiiiii/brotector) ✅ (with [CDP-Patches](https://github.com/Kaliiiiiiiiii-Vinyzu/CDP-Patches/))
- [Cloudflare](https://cloudflare.com/) ✅
- [Kasada](https://www.kasada.io/) ✅
- [Akamai](https://www.akamai.com/products/bot-manager/) ✅
- [Shape/F5](https://www.f5.com/) ✅
- [Bet365](https://bet365.com/) ✅
- [Datadome](https://datadome.co/products/bot-protection/) ✅
- [Fingerprint.com](https://fingerprint.com/products/bot-detection/) ✅
- [CreepJS](https://abrahamjuliot.github.io/creepjs/) ✅
- [Sannysoft](https://bot.sannysoft.com/) ✅
- [Incolumitas](https://bot.incolumitas.com/) ✅
- [IPHey](https://iphey.com/) ✅
- [Browserscan](https://browserscan.net/) ✅
- [Pixelscan](https://pixelscan.net/) ✅

Each entry has a live test in [`src/tests/patchright_stealth.rs`](src/tests/patchright_stealth.rs), copied into the patched crate as `tests/patchright_stealth.rs`. They use the Best Practice setup above, depend on the network and the reputation of your IP, and are ignored by default:
```bash
cd playwright-rust && cargo test -p patchright-rs --test patchright_stealth -- --ignored --test-threads=1
```
The anti-bot vendors are checked through sites they protect (Kasada: hyatt.com, Akamai: adidas.com, Shape/F5: southwest.com, Datadome: tripadvisor.com, Cloudflare: a Turnstile challenge page). Brotector is served from its repository, since its GitHub Page is gone.

---

## Documentation and API Reference
See the [playwright-rs Documentation](https://docs.rs/playwright-rs) and the original [Playwright API Reference](https://playwright.dev/docs/api/class-playwright)

## Extended Patchright API
#### **`evaluate`**, **`evaluate_handle`** and **`evaluate_all`** Methods <sub>(`Frame`, `Page`, `Locator`, `Worker`, `JSHandle`, `ElementHandle`)</sub>
- Run in an isolated Execution Context by default. Rust has no keyword arguments, so instead of `isolated_context=False`, wrap the call in `main_world(...)` (or `with_isolated_context(bool, ...)`) to choose the Execution Context (Main/Isolated).
```diff
  // Isolated Execution Context (default)
  page.evaluate("() => document.title", None::<&()>).await?;
  // Main Execution Context
+ patchright_rs::main_world(page.evaluate("() => document.title", None::<&()>)).await?;
```

#### **`focus_control`** Option <sub>(`Browser::new_context_with_options`, `BrowserType::launch_persistent_context_with_options`)</sub>
```diff
  BrowserContextOptions::builder()
+     .focus_control(true)
      .build()
```

#### **`patchright_init_script`** Option <sub>(`Route::continue_`, `Route::fallback`)</sub>
```diff
  ContinueOptions::builder()
+     .patchright_init_script(true)
      .build()
```

### Differences from Patchright-Python
- `route.fallback()` no longer sends `continue` immediately: overrides accumulate and the route is continued once after the last handler, as Playwright's other clients do. playwright-rs continued on every fallback, which breaks once the init-script route is added.
- Not ported, because playwright-rs implements them differently: the `wait_for_url`, `all_headers` and `video` tweaks, and `connect`'s `wsEndpoint` rename (playwright-rs connects over WebSocket itself).

---

## Bugs
#### The bugs are documented in the [Patchright Driver Repository](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright#bugs).

---

### TODO
#### The TODO is documented in the [Patchright Driver Repository](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright#todo).


---

## Development

Deployment of new Patchright versions are automatic, but bugs due to Playwright codebase changes may occur. Fixes for these bugs might take a few days to be released.

The [release workflow](.github/workflows/patchright_release.yml) runs hourly: [`src/utils/release_version_check.sh`](src/utils/release_version_check.sh) picks the newest playwright-rs release whose bundled Playwright version has a matching `patchright-core`, [`src/patch_rust_package.rs`](src/patch_rust_package.rs) patches it, and the result is pushed as the `v<version>` tag of the same version if that tag does not exist yet.

To build locally:
```bash
git clone --depth 1 --branch v0.19.0 https://github.com/padamson/playwright-rust
patchright_core_version=1.63.0 patchright_version=0.19.0 cargo run
cd playwright-rust && cargo build -p patchright-rs
```

---

## Copyright and License
Patchright © [Vinyzu](https://github.com/Vinyzu/), playwright-rs © [Paul Adamson](https://github.com/padamson)

The patched patchright-rs crate in the release tags is licensed [Apache 2.0](https://choosealicense.com/licenses/apache-2.0/), like the code it is derived from. The patcher in this repository is [MIT](LICENSE) licensed.

---

## Disclaimer

This repository is provided for **educational purposes only**. \
No warranties are provided regarding accuracy, completeness, or suitability for any purpose. **Use at your own risk**—the authors and maintainers assume **no liability** for **any damages**, **legal issues**, or **warranty breaches** resulting from use, modification, or distribution of this code.\
**Any misuse or legal violations are the sole responsibility of the user**.
