# Benchmarking

This document describes how to run and track performance benchmarks for playwright-rust.

## Overview

We use [criterion.rs](https://github.com/bheisler/criterion.rs) for benchmarking. Criterion provides:
- Statistical analysis with confidence intervals
- Automatic comparison against previous runs
- HTML reports with charts and graphs
- Baseline management for long-term tracking
- Gnuplot integration for visualization

## Quick Start

### Run all benchmarks

```bash
cargo bench
```

This runs all benchmarks and compares against the previous run.

### Run specific benchmark

```bash
# Run only GUID operation benchmarks
cargo bench --bench guid_operations

# Run only page operation benchmarks
cargo bench --bench page_operations

# Run only browser operation benchmarks
cargo bench --bench browser_operations
```

### View results

After running benchmarks, open the HTML report:

```bash
open target/criterion/report/index.html
```

## Baseline Management

Baselines allow you to track performance over time and compare against specific commits.

### Save a baseline

```bash
# Save a baseline with a descriptive name
cargo bench -- --save-baseline before-guid-optimization

# Or use a commit hash for reference
cargo bench -- --save-baseline baseline-c3c16f6
```

This saves the current benchmark results as a named baseline for future comparison.

### Compare against a baseline

```bash
# Compare current performance against a saved baseline
cargo bench -- --baseline before-guid-optimization
```

Criterion will show percentage changes compared to the baseline.

### List saved baselines

Baselines are stored in `target/criterion/*/base/`. Each benchmark stores its own baseline:

```bash
# List all baselines
find target/criterion -type d -name base

# Or check a specific benchmark
ls target/criterion/guid_operations/*/
```

## Tracked Benchmarks

### GUID Operations
- **String Clone**: Time to clone a GUID string
- **Arc<str> Clone**: Time to clone an Arc<str> GUID
- **HashMap Lookups**: Comparison of String vs Arc<str> in HashMaps

**Target**: Arc<str> should be 5x+ faster for cloning, 2x+ faster for lookups

### Page Operations
- Page navigation (goto, reload)
- Element queries (locator operations)
- JavaScript evaluation
- Screenshots

### Browser Operations
- Browser launch times (Chromium, Firefox, WebKit)
- Browser context creation
- Page creation

## Visualization with Gnuplot

If you have gnuplot installed, criterion will generate SVG plots in addition to HTML reports:

```bash
# macOS
brew install gnuplot

# Linux
sudo apt-get install gnuplot
```

Criterion automatically detects gnuplot and generates plots. View them in the HTML report at `target/criterion/report/index.html`.

## Optimization Workflow

When implementing performance optimizations, use this workflow to track improvements:

### 1. Save baseline before changes

```bash
# Record current performance
cargo bench -- --save-baseline before-guid-optimization
```

### 2. Implement optimization

Make your code changes.

### 3. Compare against baseline

```bash
# See the performance impact
cargo bench -- --baseline before-guid-optimization
```

Criterion will show percentage changes for each benchmark. Look for:
- 🟢 Green = Performance improved (faster)
- 🔴 Red = Performance regressed (slower)
- ⚪ White = No significant change

### 4. Document results

In the implementation plan, document:
- Baseline metrics (before)
- Optimized metrics (after)
- Percentage improvement
- Any trade-offs

### 5. Commit changes

Once satisfied with the improvements, commit your code.

## Benchmark Structure

Benchmarks are organized in `crates/playwright/benches/`:

- `guid_operations.rs` - GUID string performance (clone, HashMap operations)
- `page_operations.rs` - Page navigation, locators, JavaScript evaluation
- `browser_operations.rs` - Browser launch, context creation

Each benchmark uses criterion groups for organization:

```rust
fn benchmark_guid_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("guid_operations");

    group.bench_function("string_clone", |b| {
        // benchmark code
    });

    group.finish();
}
```

## Performance Targets

Performance optimization goals are documented in the relevant implementation plan slices. When working on performance improvements:

1. Check the implementation plan for specific targets
2. Save a baseline before starting work
3. Implement the optimization
4. Compare against the baseline to measure improvement
5. Document actual results in the implementation plan

## Tips

- Benchmarks run multiple iterations to reduce noise
- Close other applications for consistent results
- Browser benchmarks are slow - use longer sample times
- Use `--sample-size` to adjust iteration count:
  ```bash
  cargo bench -- --sample-size 10
  ```
- Filter benchmarks with patterns:
  ```bash
  cargo bench guid  # Runs only benchmarks matching "guid"
  ```

## CI Integration

Criterion-based benchmarks are not run in CI [for reasons](https://bheisler.github.io/criterion.rs/book/faq.html#how-should-i-run-criterionrs-benchmarks-in-a-ci-pipeline).
