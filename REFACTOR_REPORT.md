# hftbtpro Single-File E2E Refactor Report

Commit: `f4774e65d0694ae6f46db70c9660dc4804b8e699`

## 1. Refactor Plan

1. Replace ad-hoc CLI matching with a table-driven command registry.
2. Keep one physical Rust file: `hftbt.rs`.
3. Organize the file into internal modules only:
   - `bootstrap`
   - `command_registry`
   - `config`
   - `data_io`
   - `binance_pipeline`
   - `aga_stack`
   - `hft_engine_adapter`
   - `audit`
   - `report`
   - `utils`
4. Centralize config into typed structs:
   - paths
   - model
   - execution
   - risk
5. Replace normal-path panics with structured `AppError` and `AppResult<T>`.
6. Preserve E2E flow:
   - tick input
   - feature generation
   - demo/trained-weight interface
   - AGA-style inference
   - HMM posterior
   - signal policy
   - backtest
   - audit
   - report
7. Preserve compatibility by keeping old command names as aliases.
8. Keep no external Rust dependencies.

## 2. Deletion List by Risk

### Safe Delete

| Item | Reason |
|---|---|
| Huge direct `match command.as_str()` style | Replaced by command registry. |
| Duplicate command aliases as separate branches | Replaced by canonical command + aliases. |
| Debug print spam | Replaced by stable status lines. |
| User-specific hardcoded paths | Replaced by config defaults and CLI/env overrides. |
| Runtime `.unwrap()` / `.expect()` / `panic!()` | Replaced with `Result` and structured errors. |
| `remove(0)`-style rolling buffer | Replaced with `VecDeque`. |

### Needs Test

| Item | Reason |
|---|---|
| Placeholder model training | Current `aga-train` writes deterministic demo weights; real training should be added later. |
| Binance network collector | Current connector/collector are offline launch shims. Real exchange API integration needs credentials/network tests. |
| Queue model | Backtest has latency, fee, slippage, inventory, fill/PnL accounting; full queue-position model needs live hftbacktest integration. |
| Real learned weights | Loader validates demo weights only; binary/JSON trained weights format should be defined before production. |

### Do Not Delete

| Item | Reason |
|---|---|
| CLI aliases | Required for backward compatibility. |
| Bootstrap/build/check/test/release commands | Required by E2E appliance workflow. |
| AGA pipeline commands | Core modeling path. |
| Audit/report commands | Required for regression and reproducibility. |
| Single-file internal modules | Required by architecture constraint. |

## 3. Cleaned Code

The cleaned single-file Rust code is in:

```text
hftbt.rs
```

Static scan performed in local sandbox:

```text
lines: 1547
unwrap(: 0
expect(: 0
panic!(: 0
TODO: 0
required modules: present
```

Note: the local sandbox did not have `rustc` or `cargo`, so compile verification must be run in an environment with Rust installed.

## 4. Compatibility Matrix

| Old / Alias | Canonical Command |
|---|---|
| `-h`, `--help` | `help` |
| `compat`, `compatibility` | `commands` |
| `smoke` | `verify` |
| `harness`, `cargo-harness` | `bootstrap` |
| `cargo-check` | `check` |
| `cargo-build` | `build` |
| `cargo-test` | `test` |
| `build-release`, `cargo-release` | `release` |
| `proof`, `e2e-proof` | `portable-proof` |
| `binance`, `binance-auto`, `binance-sample` | `binance-smoke` |
| `binance-normalize` | `binance-prepare` |
| `run-collector`, `collector-smoke` | `collector` |
| `run-connector`, `connector-smoke` | `connector` |
| `aga` | `aga-auto` |
| `prepare`, `features` | `aga-prepare` |
| `train`, `aga-fit` | `aga-train` |
| `infer`, `signals` | `aga-infer` |
| `backtest`, `hft`, `hftbacktest` | `aga-backtest` |
| `audit` | `aga-audit` |
| `model-proof` | `aga-proof` |
| `lobster-convert` | `aga-lobster-convert` |
| `lobframe-convert` | `aga-lobframe-convert` |
| `backtest-report` | `report` |

## 5. Test Plan

Run locally after installing Rust stable:

```bash
cargo check
cargo test
cargo run -- verify
cargo run -- commands
cargo run -- binance-smoke --input target/hftbtpro/ticks.csv --rows 256
cargo run -- aga-prepare --input target/hftbtpro/ticks.csv
cargo run -- aga-train
cargo run -- aga-infer
cargo run -- aga-backtest
cargo run -- aga-audit
cargo run -- report
cargo run -- portable-proof
cargo run -- release
```

Standalone `rustc` compile:

```bash
rustc --edition=2021 hftbt.rs -O -o hftbtpro
./hftbtpro portable-proof
```

## 6. Regression Checklist

- [ ] One physical Rust file remains: `hftbt.rs`.
- [ ] `cargo check` passes.
- [ ] `cargo test` passes.
- [ ] `rustc --edition=2021 hftbt.rs` passes.
- [ ] `portable-proof` creates ticks, features, weights, signals, backtest, audit, summary.
- [ ] `aga-auto` runs E2E on sample ticks.
- [ ] `aga-infer` produces probability rows summing to 1.
- [ ] `aga-audit` validates timestamp monotonicity, probabilities, posterior, gate.
- [ ] `aga-backtest` writes PnL/fill/slippage/latency report.
- [ ] `binance-smoke` creates canonical tick CSV.
- [ ] No user-specific hardcoded paths.
- [ ] No runtime `.unwrap()`, `.expect()`, or `panic!()` in normal paths.
- [ ] Backward-compatible aliases resolve to canonical commands.
