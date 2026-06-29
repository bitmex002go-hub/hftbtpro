# hftbtpro — Complete E2E Production Pipeline Blueprint

Goal: evolve `hftbt.rs` from an offline single-file scaffold into a complete Rust E2E HFT research appliance while keeping one physical Rust file.

Status basis: `hftbt.rs` currently has a clean command registry, config, canonical CSV, AGA-style demo inference, simple backtest, audit, and report path. The remaining work is to close the production gaps: live/historical ingestion, local order book reconstruction, hftbacktest-compatible event export, real latency/queue/fill model, training/evaluation split, and CI reproducibility.

---

## 0. Non-negotiable invariants

1. One physical Rust file: `hftbt.rs`.
2. Internal modules are allowed; external source files are not.
3. Every stage must write deterministic artifacts into `target/hftbtpro/` by default.
4. Every command must be replayable with explicit input/output paths.
5. No future leakage: inference cannot read labels or future returns.
6. Backtest must report latency, queue, fill, slippage, fee, inventory, and PnL.
7. Audit must fail closed, not silently pass bad data.
8. Offline mode must always work without API keys.
9. Live/data-download mode must be gated and explicit.
10. Compatibility aliases must remain.

---

## 1. Full pipeline map

```text
SOURCE
  ├─ offline sample generator
  ├─ Binance live websocket collector
  ├─ Binance historical/raw JSONL import
  ├─ LOBSTER CSV import
  └─ LOBFrame/benchmark import

RAW EVENT LOG
  ├─ raw_market.jsonl
  ├─ raw_trades.jsonl
  └─ raw_depth.jsonl

NORMALIZATION
  ├─ canonical_event.csv
  ├─ canonical_tick.csv
  ├─ hftbacktest_event.csv
  └─ schema/version metadata

BOOK RECONSTRUCTION
  ├─ local order book from snapshot + diff-depth stream
  ├─ gap detection and resync
  ├─ top-N L2 snapshots
  ├─ trades aligned by exchange/local timestamp
  └─ feed-latency series

FEATURE/LABEL
  ├─ microstructure features
  ├─ volatility/frequency features
  ├─ OFI/depth/imbalance features
  ├─ train labels only
  └─ inference features without labels

MODEL
  ├─ train/validate/test split
  ├─ deterministic demo weights
  ├─ trained weights format
  ├─ FineEncoder
  ├─ CoarseEncoder
  ├─ AdaptiveGate
  ├─ CausalAttention
  ├─ Conditional Flow emission
  ├─ Neural HMM posterior
  └─ signal head

SIGNAL POLICY
  ├─ BUY/SELL/HOLD/CANCEL
  ├─ quote width
  ├─ order size
  ├─ high-volatility kill switch
  └─ inventory-aware skew

EXECUTION/BACKTEST
  ├─ hftbacktest adapter export
  ├─ internal replay engine
  ├─ feed latency
  ├─ order entry latency
  ├─ order response latency
  ├─ queue position model
  ├─ partial/no-partial fill model
  ├─ fee/slippage
  └─ PnL attribution

AUDIT
  ├─ schema audit
  ├─ timestamp audit
  ├─ book-cross audit
  ├─ depth gap audit
  ├─ feature leakage audit
  ├─ probability invariant audit
  ├─ latency audit
  ├─ fill sanity audit
  ├─ inventory audit
  └─ command coverage audit

REPORT/CI
  ├─ JSON report
  ├─ markdown summary
  ├─ reproducibility manifest
  ├─ cargo check/test
  ├─ rustc single-file compile
  └─ GitHub Actions matrix
```

---

## 2. Current gaps that must be closed

| Area | Current state | Required production state | Priority |
|---|---|---|---|
| Live Binance ingestion | offline shim | websocket stream, ping/pong, reconnect, raw JSONL, depth/trade/bookTicker streams | P0 |
| Local order book | top-of-book sample only | snapshot + diff-depth replay, update ID gap detection, resync | P0 |
| hftbacktest event format | not exported | `ev, exch_ts, local_ts, px, qty, order_id, ival, fval` export | P0 |
| Data validation | basic monotonic check | exch/local timestamp order, positive feed latency, crossed-book, gap checks | P0 |
| Latency model | constant only | feed latency + order entry + order response; interpolation from latency CSV | P0 |
| Queue model | simplified fill | risk-averse and probabilistic queue model | P0 |
| Fill model | simple taker-like fill | maker/taker, partial/no-partial, queue-aware fill | P0 |
| Training | demo weights | walk-forward split, loss, metrics, model save/load | P1 |
| Evaluation | none/minimal | train/val/test, confusion, calibration, hit-rate, turnover, PnL | P1 |
| Feature store | CSV only | versioned feature schema + manifest + no-leakage proof | P1 |
| Report | compact | run manifest, config, data coverage, model metrics, backtest metrics, audit failures | P1 |
| CI | manual | GitHub Actions: check, test, rustc, portable-proof | P1 |
| Live trading | not required now | paper trading only, no real orders until backtest/live discrepancy audit | P2 |

---

## 3. Command pipeline target

### P0 commands

```bash
cargo run -- binance-live \
  --symbol BTCUSDT \
  --streams depth@100ms,trade,bookTicker \
  --raw target/hftbtpro/raw_market.jsonl \
  --duration-sec 3600

cargo run -- book-build \
  --raw target/hftbtpro/raw_market.jsonl \
  --snapshot target/hftbtpro/snapshot.json \
  --ticks target/hftbtpro/ticks.csv \
  --events target/hftbtpro/hft_events.csv

cargo run -- validate-data \
  --events target/hftbtpro/hft_events.csv \
  --ticks target/hftbtpro/ticks.csv \
  --report target/hftbtpro/data_audit.json

cargo run -- aga-prepare \
  --input target/hftbtpro/ticks.csv \
  --features target/hftbtpro/features.csv

cargo run -- aga-train \
  --features target/hftbtpro/features.csv \
  --weights target/hftbtpro/weights.bin

cargo run -- aga-infer \
  --features target/hftbtpro/features.csv \
  --weights target/hftbtpro/weights.bin \
  --signals target/hftbtpro/signals.csv

cargo run -- aga-backtest \
  --events target/hftbtpro/hft_events.csv \
  --signals target/hftbtpro/signals.csv \
  --latency target/hftbtpro/latency.csv \
  --backtest-report target/hftbtpro/backtest.json

cargo run -- aga-audit \
  --report target/hftbtpro/audit.json

cargo run -- report \
  --workdir target/hftbtpro
```

### One-shot target

```bash
cargo run -- e2e-full \
  --mode offline|historical|live \
  --symbol BTCUSDT \
  --workdir target/hftbtpro \
  --strict
```

---

## 4. Required artifact schema

### 4.1 Raw event JSONL

```json
{"schema":"hftbtpro.raw.v1","local_ts_ns":0,"exchange_ts_ns":0,"stream":"btcusdt@depth@100ms","payload":{}}
```

Required fields:

- `schema`
- `local_ts_ns`
- `exchange_ts_ns`
- `stream`
- `payload`
- `source`
- `symbol`

### 4.2 Canonical tick CSV

```text
ts_ns,local_ts_ns,bid_px,ask_px,bid_qty,ask_qty,trade_px,trade_qty,trade_side,update_id
```

### 4.3 hftbacktest event CSV

```text
ev,exch_ts,local_ts,px,qty,order_id,ival,fval
```

Mapping:

- depth bid update → `BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT`
- depth ask update → `SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT`
- trade buy/sell → `BUY_EVENT/SELL_EVENT | TRADE_EVENT | EXCH_EVENT | LOCAL_EVENT`
- `ival` stores update ID when available
- `fval` stores optional auxiliary value such as queue hint

### 4.4 Feature CSV

```text
ts_ns,mid,spread_bps,mid_ret_bps,imbalance,depth,ofi,micro_bps,trade_sign,trade_qty_norm,sigma,lambda,label,split
```

Rules:

- `label` can exist only for train/validation/test feature files.
- inference feature files must have empty label column.
- all rolling features must use `t` and past data only.

### 4.5 Signal CSV

```text
ts_ns,prob_down,prob_neutral,prob_up,regime,gate,confidence,signal,quote_width_bps,order_size,high_vol_prob,post0,post1,post2,post3
```

Invariants:

- `prob_down + prob_neutral + prob_up = 1 ± 1e-6`
- `post0 + post1 + post2 + post3 = 1 ± 1e-6`
- `gate ∈ [0,1]`
- `confidence ∈ [0,1]`

### 4.6 Backtest report JSON

```json
{
  "schema":"hftbtpro.backtest.v1",
  "rows":0,
  "fills":0,
  "fill_ratio":0.0,
  "final_pnl":0.0,
  "max_abs_inventory":0.0,
  "avg_slippage_bps":0.0,
  "latency_p50_ns":0,
  "latency_p95_ns":0,
  "latency_p99_ns":0,
  "queue_model":"risk_averse|probabilistic",
  "fill_model":"no_partial|partial"
}
```

---

## 5. Internal module upgrades needed in `hftbt.rs`

### 5.1 `data_io`

Add:

- `RawEvent`
- `DepthUpdate`
- `TradeEvent`
- `BookTickerEvent`
- `HftEvent`
- `read_jsonl_raw_events`
- `write_hft_events`
- `read_hft_events`
- schema/version checks

### 5.2 `binance_pipeline`

Add:

- websocket URL builder
- subscription command builder
- ping/pong policy
- reconnect policy
- raw JSONL writer
- REST depth snapshot loader
- stream gap handling
- `binance-live`
- `binance-import-raw`
- `binance-snapshot`

### 5.3 `book_builder`

New internal module required.

Responsibilities:

- maintain bid/ask maps by price tick
- apply snapshot
- apply diff-depth updates
- detect update ID gaps
- produce top-N book snapshots
- fuse bookTicker + depth where available
- output canonical tick/event streams

### 5.4 `hft_engine_adapter`

Upgrade:

- consume hftbacktest-style event CSV, not only simplified ticks
- add exchange model enum:
  - `NoPartialFill`
  - `PartialFill`
- add queue model enum:
  - `RiskAverse`
  - `ProbQueue`
- add order lifecycle:
  - New
  - PendingSubmit
  - Resting
  - PartiallyFilled
  - Filled
  - PendingCancel
  - Canceled
  - Rejected
- add maker/taker distinction
- track request/exchange/response timestamps

### 5.5 `latency`

New internal module required.

Responsibilities:

- constant latency
- feed latency derived from `local_ts - exch_ts`
- interpolated order latency from CSV
- p50/p95/p99 reporting
- negative-latency correction option

### 5.6 `aga_stack`

Upgrade:

- train/val/test split
- no-leakage feature builder
- real weight serialization
- loss calculation
- calibration metrics
- model version metadata
- deterministic seed
- walk-forward mode

### 5.7 `audit`

Upgrade:

- raw JSONL schema audit
- sequence ID audit
- book gap audit
- crossed-book audit
- timestamp audit
- feed-latency audit
- no-leakage audit
- command coverage audit
- artifact presence audit

### 5.8 `report`

Upgrade:

- Markdown + JSON report
- include run manifest
- include command versions
- include config
- include data span
- include training/eval/backtest metrics
- include audit failures and warnings

---

## 6. Production E2E implementation order

### Phase P0 — Make E2E real, not demo

1. Add `HftEvent` format.
2. Add `book_builder` module.
3. Add `latency` module.
4. Upgrade backtest engine to event replay.
5. Add queue-aware fill simulation.
6. Add `e2e-full` command.
7. Add strict data audit.
8. Add GitHub Actions CI.

Exit criteria:

```bash
cargo check
cargo test
cargo run -- portable-proof
cargo run -- e2e-full --mode offline --strict
rustc --edition=2021 hftbt.rs -O -o hftbtpro
./hftbtpro e2e-full --mode offline --strict
```

### Phase P1 — Make research valid

1. Implement walk-forward split.
2. Add real training loop or deterministic external-weight loader.
3. Add evaluation metrics.
4. Add signal calibration.
5. Add PnL attribution.
6. Add benchmark importers: LOBSTER/LOBFrame.

Exit criteria:

```bash
cargo run -- aga-train --split walk-forward
cargo run -- aga-eval
cargo run -- aga-backtest --queue-model prob
cargo run -- report
```

### Phase P2 — Make live/paper robust

1. Add live Binance websocket collector.
2. Add raw JSONL replay.
3. Add live-vs-backtest discrepancy report.
4. Add paper trading only.
5. Add kill switches.

Exit criteria:

```bash
cargo run -- binance-live --duration-sec 3600 --raw raw.jsonl
cargo run -- e2e-full --mode historical --raw raw.jsonl --strict
cargo run -- live-replay-compare
```

---

## 7. Acceptance checklist

- [ ] `hftbt.rs` remains one physical Rust file.
- [ ] `cargo check` passes.
- [ ] `cargo test` passes.
- [ ] direct `rustc` compile passes.
- [ ] `portable-proof` runs full offline path.
- [ ] `e2e-full --mode offline --strict` runs full offline path.
- [ ] raw events can be normalized into canonical events.
- [ ] order book snapshot + diff-depth reconstruction works.
- [ ] sequence gaps trigger resync/failure.
- [ ] hftbacktest event CSV is generated.
- [ ] latency model produces p50/p95/p99.
- [ ] queue model changes fill probability.
- [ ] partial/no-partial fill modes are selectable.
- [ ] feature labels are absent in inference mode.
- [ ] probability and posterior invariants hold.
- [ ] audit fails on corrupted timestamps.
- [ ] report includes all artifacts and config.
- [ ] CI runs check/test/portable-proof.

---

## 8. Source notes

Design is aligned with:

- hftbacktest data/event model: https://hftbacktest.readthedocs.io/en/latest/data.html
- hftbacktest latency model: https://hftbacktest.readthedocs.io/en/latest/latency_models.html
- hftbacktest order fill / queue model assumptions: https://hftbacktest.readthedocs.io/en/latest/order_fill.html
- Binance websocket stream and local book procedure: https://developers.binance.com/docs/binance-spot-api-docs/web-socket-streams
- LOBFrame/Deep Limit Order Book Forecasting benchmark idea: https://arxiv.org/abs/2403.09267
