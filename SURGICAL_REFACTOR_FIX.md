# Surgical refactor fix plan

The previous refactor was wrong because it replaced the real fused `hftbt.rs` with a short scaffold. The uploaded `hftbt.rs` is the real target and must be preserved.

## Correct rule

Do not rewrite `hftbt.rs` from scratch.

The real file contains:

- `single_file_bootstrap`
- generated Cargo workspace harness
- edition 2024 crate manifests
- hftbacktest library path
- py-hftbacktest path
- collector and connector crates
- embedded Python helpers
- Binance data pipeline
- AGA commands
- portable proof
- smoke tests

A valid refactor must preserve every externally visible command and every embedded script path.

## Immediate restoration step

Replace repo `hftbt.rs` with the uploaded original `hftbt.rs` before any refactor.

Local command:

```bash
cp /path/to/uploaded/hftbt.rs ./hftbt.rs
git add hftbt.rs
git commit -m "restore real fused hftbt single-file appliance"
git push
```

## Safe refactor order

### Phase 0: restore behavior

Run:

```bash
rustc --edition=2024 hftbt.rs -o /tmp/hftbt
/tmp/hftbt --help
/tmp/hftbt harness
/tmp/hftbt check
/tmp/hftbt aga-auto
/tmp/hftbt portable-proof --skip-e2e
```

No structural rewrite until this passes.

### Phase 1: command registry without behavior change

Only refactor `match command.as_str()` into a registry table.

Do not change handler functions.
Do not rename commands.
Do not delete aliases.
Do not remove embedded Python scripts.

### Phase 2: config cleanup

Replace hardcoded defaults with env-overridable config while keeping old defaults when env is absent.

Example:

```rust
root: env::var("HFTBT_OFFLINE_DB").unwrap_or_else(|_| "/home/aiman/hftbt_offline_db".to_string())
```

### Phase 3: runtime safety

Replace normal-path `unwrap` only where it is demonstrably reachable from CLI input.

Do not touch proc-macro/internal code until compile tests pass.

### Phase 4: AGA stack cleanup

Refactor only inside embedded AGA module.

Preserve:

```text
aga
aga-sample
aga-prepare
aga-train
aga-infer
aga-backtest
aga-audit
aga-auto
aga-proof
aga-lobster-convert
aga-lobframe-convert
```

### Phase 5: E2E expansion

Add new P0 commands as additive aliases only. Do not replace existing Binance/hftbacktest commands.

## Commands that must remain working

```text
verify
fmt
check
test
doctest
build
release
build-py
portable-proof
binance-auto
binance-db
binance-convert
binance-l1-prepare
binance-l2-prepare
binance-tick-l2
binance-report
binance-pretrain
binance-production-pretrain-e2e
binance-neural-train
aga-prepare
aga-train
aga-infer
aga-backtest
aga-audit
aga-auto
aga-proof
binance-vectorization-audit
binance-lifecycle-audit
binance-data-tiers
binance-smoke
binance-l2-smoke
binance-l1-smoke
collector
connector
harness
clean
```

## Acceptance

A refactor is accepted only if:

1. `rustc --edition=2024 hftbt.rs` passes.
2. `/tmp/hftbt --help` prints all old commands.
3. `/tmp/hftbt harness` recreates external workspace.
4. `/tmp/hftbt check` passes in the generated harness.
5. `/tmp/hftbt aga-auto` still runs.
6. `/tmp/hftbt portable-proof --skip-e2e` still runs.

## Important

The repository currently needs restoration from the uploaded real file before any further code refactor. The correct approach is restore first, then surgical refactor in small commits.
