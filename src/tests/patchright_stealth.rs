//! Live checks against the detection sites Patchright claims to pass.
//!
//! They hit third-party sites, so the results depend on the network and on the
//! reputation of the egress IP. Ignored by default; run them with a real Chrome:
//!
//! ```sh
//! cargo test -p patchright-rs --test patchright_stealth -- --ignored --test-threads=1
//! ```

use patchright_rs::Playwright;
use patchright_rs::protocol::{BrowserContext, BrowserContextOptions, FetchOptions, FulfillOptions, Page};
use serde_json::Value;
use std::time::{Duration, Instant};

const BROTECTOR_COMMIT: &str = "98b330999e9928a226ca0c8254dddf909d1583bd";

const REAL_PAGE: &str = r#"() => {
    const text = document.body ? document.body.innerText : "";
    const blocked = /access denied|just a moment|请稍候|attention required|pardon our interruption|verify you are human|request unsuccessful|you have been blocked/i;
    return text.length > 500
        && !blocked.test(document.title + " " + text.slice(0, 2000))
        && !document.querySelector('iframe[src*="captcha-delivery.com"]');
}"#;

struct Session {
    _playwright: Playwright,
    context: BrowserContext,
    page: Page,
    _profile: tempfile::TempDir,
}

impl Session {
    async fn start() -> Self {
        let playwright = Playwright::launch().await.expect("launch playwright");
        let profile = tempfile::tempdir().expect("profile dir");
        let context = playwright
            .chromium()
            .launch_persistent_context_with_options(
                profile.path().to_string_lossy().to_string(),
                BrowserContextOptions::builder()
                    .channel("chrome".to_string())
                    .headless(false)
                    .no_viewport(true)
                    .build(),
            )
            .await
            .expect("launch chrome");
        let page = context.new_page().await.expect("new page");
        Self { _playwright: playwright, context, page, _profile: profile }
    }

    async fn goto(&self, url: &str) {
        self.page.goto(url, None).await.expect("goto");
    }

    async fn eval(&self, js: &str) -> Value {
        self.page.evaluate(js, None::<&()>).await.expect("evaluate")
    }

    async fn text(&self) -> String {
        let text = self.eval("() => document.body ? document.body.innerText : ''").await;
        text.as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    async fn wait_for(&self, predicate: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(Value::Bool(true)) = self.page.evaluate(predicate, None::<&()>).await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    async fn assert_not_blocked(&self, url: &str) {
        self.goto(url).await;
        let passed = self.wait_for(REAL_PAGE, Duration::from_secs(45)).await;
        let title = self.eval("() => document.title").await;
        let text = self.text().await;
        assert!(passed, "blocked on {url}: title={title} text={}", &text.chars().take(300).collect::<String>());
    }

    async fn close(self) {
        self.context.close().await.ok();
    }
}

fn collect_failures(path: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => map.iter().for_each(|(k, v)| collect_failures(&format!("{path}.{k}"), v, out)),
        Value::Array(items) => items.iter().enumerate().for_each(|(i, v)| collect_failures(&format!("{path}[{i}]"), v, out)),
        Value::String(s) if s == "FAIL" => out.push(path.to_string()),
        _ => {}
    }
}

#[tokio::test]
#[ignore = "live site"]
async fn brotector() {
    let s = Session::start().await;
    s.page
        .route("https://brotector.patchright.test/**", |route| async move {
            let path = route.request().url().trim_start_matches("https://brotector.patchright.test/").to_string();
            let url = format!("https://raw.githubusercontent.com/kaliiiiiiiiii/brotector/{BROTECTOR_COMMIT}/{path}");
            let response = route.fetch(FetchOptions::builder().url(url).build()).await?;
            let content_type = match path.rsplit('.').next() {
                Some("html") => "text/html",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                _ => "application/octet-stream",
            };
            route
                .fulfill(FulfillOptions::builder().body(response.body().to_vec()).content_type(content_type).build())
                .await
        })
        .await
        .unwrap();
    s.goto("https://brotector.patchright.test/index.html").await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    s.page.locator("#clickHere").click(None).await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    let detections = s
        .eval("() => [...document.querySelectorAll('#detections tr')].slice(2).map(r => r.innerText.replace(/\\s+/g, ' ').slice(0, 200))")
        .await;
    assert_eq!(detections, Value::Array(vec![]), "brotector detections");
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn cloudflare() {
    let s = Session::start().await;
    s.goto("https://www.scrapingcourse.com/cloudflare-challenge").await;
    let turnstile = r#"() => { const i = document.querySelector('input[name="cf-turnstile-response"]'); if (!i) return null; const r = i.parentElement.getBoundingClientRect(); return r.height > 0 ? [r.x, r.y, r.height] : null; }"#;
    let passed = r#"() => document.title.includes("ScrapingCourse")"#;
    let mut ok = false;
    for _ in 0..4 {
        if s.wait_for(passed, Duration::from_secs(10)).await {
            ok = true;
            break;
        }
        if let Value::Array(r) = s.eval(turnstile).await {
            let (x, y, h) = (r[0].as_f64().unwrap(), r[1].as_f64().unwrap(), r[2].as_f64().unwrap());
            s.page.mouse().click(x + 30.0, y + h / 2.0, None).await.unwrap();
        }
    }
    assert!(ok, "cloudflare challenge not passed: {}", s.text().await);
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn kasada() {
    let s = Session::start().await;
    s.assert_not_blocked("https://www.hyatt.com/").await;
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn akamai() {
    let s = Session::start().await;
    s.assert_not_blocked("https://www.adidas.com/us").await;
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn shape() {
    let s = Session::start().await;
    s.assert_not_blocked("https://www.southwest.com/").await;
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn bet365() {
    let s = Session::start().await;
    s.assert_not_blocked("https://www.bet365.com/").await;
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn datadome() {
    let s = Session::start().await;
    s.assert_not_blocked("https://www.tripadvisor.com/").await;
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn fingerprint() {
    let s = Session::start().await;
    s.goto("https://demo.fingerprint.com/web-scraping").await;
    s.page.locator("button[class*=searchButton]").click(None).await.unwrap();
    let found = s.wait_for("() => document.body.innerText.includes('Price per adult')", Duration::from_secs(30)).await;
    assert!(found, "flight search blocked: {}", s.text().await);
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn creepjs() {
    let s = Session::start().await;
    s.goto("https://abrahamjuliot.github.io/creepjs/").await;
    let ready = r#"() => /\d+% headless/.test(document.body.innerText) && /\d+% stealth/.test(document.body.innerText)"#;
    assert!(s.wait_for(ready, Duration::from_secs(60)).await, "creepjs did not finish");
    let scores = s
        .eval(r#"() => { const t = document.body.innerText; return [t.match(/(\d+)% headless/)[1], t.match(/(\d+)% stealth/)[1]]; }"#)
        .await;
    assert_eq!(scores, serde_json::json!(["0", "0"]), "creepjs [headless, stealth]");
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn sannysoft() {
    let s = Session::start().await;
    s.goto("https://bot.sannysoft.com/").await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let failed = s
        .eval("() => [...document.querySelectorAll('td.failed')].map(td => (td.previousElementSibling ? td.previousElementSibling.innerText + ': ' : '') + td.innerText)")
        .await;
    assert_eq!(failed, Value::Array(vec![]), "sannysoft failed rows");
    let passed = s.eval("() => document.querySelectorAll('td.passed').length").await;
    assert!(passed.as_u64().unwrap() > 0, "sannysoft did not render");
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn incolumitas() {
    let s = Session::start().await;
    s.goto("https://bot.incolumitas.com/").await;
    let ready = "() => ['new-tests', 'detection-tests'].every(id => (document.getElementById(id) || {}).innerText?.includes('OK'))";
    assert!(s.wait_for(ready, Duration::from_secs(45)).await, "incolumitas did not finish");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let results = s
        .eval("() => ['new-tests', 'detection-tests'].map(id => JSON.parse(document.getElementById(id).innerText))")
        .await;
    let mut failed = Vec::new();
    collect_failures("", &results, &mut failed);
    // fpscanner flags any `navigator.webdriver` property, which every current Chrome has
    failed.retain(|key| !key.ends_with("fpscanner.WEBDRIVER"));
    assert!(failed.is_empty(), "incolumitas failures: {failed:?}");
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn iphey() {
    let s = Session::start().await;
    s.goto("https://iphey.com/").await;
    let ready = "() => /HARDWARE\\s+\\S/.test(document.body.innerText) && !/Checking|Loading/.test(document.body.innerText.slice(0, 1500))";
    s.wait_for(ready, Duration::from_secs(30)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let text = s.text().await;
    for section in ["HARDWARE Everything is fine", "SOFTWARE Everything is fine"] {
        assert!(text.contains(section), "iphey missing {section:?}: {}", &text.chars().take(600).collect::<String>());
    }
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn browserscan() {
    let s = Session::start().await;
    s.goto("https://www.browserscan.net/bot-detection").await;
    let ready = "() => /Test Results:\\s*\\S/.test(document.body.innerText)";
    assert!(s.wait_for(ready, Duration::from_secs(30)).await, "browserscan did not finish");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let text = s.text().await;
    assert!(text.contains("Test Results: Normal"), "browserscan: {}", &text.chars().take(600).collect::<String>());
    s.close().await;
}

#[tokio::test]
#[ignore = "live site"]
async fn pixelscan() {
    let s = Session::start().await;
    s.goto("https://pixelscan.net/bot-check").await;
    let statuses = "() => [...document.querySelectorAll('.summary-section__status, .bot-check-accordion__status')].map(e => e.innerText.trim())";
    let ready = "() => { const s = [...document.querySelectorAll('.summary-section__status, .bot-check-accordion__status')]; return s.length > 0 && s.every(e => e.innerText.trim().length > 0); }";
    assert!(s.wait_for(ready, Duration::from_secs(30)).await, "pixelscan did not finish");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let statuses = s.eval(statuses).await;
    let bad: Vec<_> = statuses.as_array().unwrap().iter().filter(|v| v.as_str() != Some("Clear")).collect();
    assert!(bad.is_empty(), "pixelscan non-clear statuses: {bad:?}");
    s.close().await;
}
