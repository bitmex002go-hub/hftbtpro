# hftbtpro

Rust E2E single-file HFT research appliance.

## Goal

- Keep one physical Rust file.
- Preserve E2E workflow.
- Clean technical debt.
- Support AGA-Neural-HMM style inference/backtest pipeline.
- Prepare for future hftbacktest integration.

## Files

- `hftbt.rs` — single-file Rust entry point / scaffold
- `META_PROMPT.md` — refactor meta-prompt for cleaning the full appliance
- `ticks.sample.csv` — tiny sample input

## Quick Start

```bash
rustc --edition=2021 hftbt.rs -O -o hftbt
./hftbt help
./hftbt aga-auto --input ticks.sample.csv --output aga_signals.csv
./hftbt aga-audit --input aga_signals.csv --report aga_audit.json
```

## Design Rule

This repository starts from a strict single-file Rust architecture. Internal modules are allowed, but the production appliance should remain one physical `.rs` file unless intentionally migrated later.
