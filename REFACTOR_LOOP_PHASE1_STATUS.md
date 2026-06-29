# Refactor loop phase 1 status

Target file names:

- `hftbt-0.rs` = original full source of truth
- `hftbt-1.rs` = refactor target

Phase 1 generated locally:

- Extracted bootstrap command dispatch out of `run()` into `dispatch_command(...)`.
- Preserved the original match arms and handler functions.
- Preserved all command aliases.
- Added Rust-side env override for the Binance DB default:
  - env: `HFTBT_OFFLINE_DB`
  - fallback: `/home/aiman/hftbt_offline_db`

Generated local artifacts:

- `hftbt-1.rs`
- `hftbt-1-phase1.patch`
- `refactor_hftbt1_phase1.py`

Why not directly update `hftbt-1.rs` through the connector:

- The file is about 1.6 MB and 39k+ lines.
- The connector cannot safely read/update that whole file as text content.
- The safe workaround is to commit a small patch/script or apply the generated local artifact manually.

Validation needed on Rust machine:

```bash
rustc --edition=2024 hftbt-1.rs -o /tmp/hftbt1
/tmp/hftbt1 --help
/tmp/hftbt1 harness
/tmp/hftbt1 check
/tmp/hftbt1 aga-auto
/tmp/hftbt1 portable-proof --skip-e2e
```

Next phase after compile passes:

1. build a true static command registry table for help/compatibility output
2. keep dispatch handler behavior unchanged
3. add config helper wrappers for hardcoded runtime paths
4. reduce unwrap only in CLI/runtime paths
