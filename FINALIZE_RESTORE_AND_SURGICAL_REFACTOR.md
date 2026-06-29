# Finalize: Restore real hftbt.rs and continue surgical refactor

This repo must use the uploaded real fused `hftbt.rs` as the source of truth.

The previous scaffold-style rewrite must not be used as the production file because it removes real behavior from the fused appliance.

## Why restore first

The real `hftbt.rs` contains the working single-file system:

- `single_file_bootstrap`
- generated external Cargo workspace
- `hftbacktest`
- `hftbacktest-derive`
- `py-hftbacktest`
- `collector`
- `connector`
- embedded Binance helpers
- embedded AGA commands
- portable proof
- smoke commands

A correct refactor is not a rewrite. It is a behavior-preserving edit over the real fused file.

## Required local restore

Copy the uploaded real file over the repo file:

```bash
cp /path/to/uploaded/hftbt.rs hftbt.rs
```

Then commit:

```bash
git add hftbt.rs
git commit -m "restore real fused hftbt single-file appliance"
git push
```

## Required validation after restore

```bash
rustc --edition=2024 hftbt.rs -o /tmp/hftbt
/tmp/hftbt --help
/tmp/hftbt harness
/tmp/hftbt check
/tmp/hftbt aga-auto
/tmp/hftbt portable-proof --skip-e2e
```

## Surgical refactor phases

### Phase 1: CLI registry only

Replace only the large `match command.as_str()` dispatch with a table-driven registry.

Rules:

- keep every existing command
- keep every alias
- call the same handler functions
- do not change embedded Python scripts
- do not change Cargo manifests
- do not change AGA logic

### Phase 2: config cleanup only

Move hardcoded defaults to env-overridable helpers without changing old defaults.

Example:

```rust
let root = env::var("HFTBT_OFFLINE_DB").unwrap_or_else(|_| "/home/aiman/hftbt_offline_db".to_string());
```

### Phase 3: runtime safety only

Replace `unwrap` only in CLI/runtime paths that can be reached by user input.

Do not touch proc-macro or generated/library internals before tests are green.

### Phase 4: AGA cleanup only

Refactor only the embedded AGA stack and keep all commands:

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

### Phase 5: additive E2E upgrades

Only add new commands after old behavior is restored and tested.

New commands must be additive aliases, not replacements.

## Done criteria

- `rustc --edition=2024 hftbt.rs` passes
- `harness` recreates workspace
- `check` passes in harness
- `aga-auto` still runs
- `portable-proof --skip-e2e` still runs
- all previous commands still appear in help
- no production behavior removed
