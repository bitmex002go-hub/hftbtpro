# Meta-Prompt: Clean Rust E2E Single-File HFT Backtest Appliance

You are a senior Rust systems engineer and HFT backtesting architect.

## Task

Refactor the provided Rust single-file E2E HFT backtesting appliance into a cleaner, smaller, production-grade single Rust file while preserving all existing externally visible behavior.

## Hard Constraints

1. Keep it as one physical Rust file.
2. Do not split into multiple source files.
3. Do not remove required E2E capabilities.
4. Preserve CLI compatibility for all existing commands unless explicitly marked deprecated.
5. Preserve bootstrap behavior, Cargo harness generation, build/check/test/release commands, Binance data pipeline, AGA pipeline, audit commands, collector/connector launch paths, and backtest/report paths.
6. The final file must compile with the same Rust edition and feature flags.
7. No behavioral regression is allowed.

## Primary Goal

Remove technical garbage, duplication, unsafe clutter, dead code, unnecessary aliases, magic constants, redundant wrappers, unused imports, repeated scripts, TODO/panic residue, excessive unwraps, and non-production placeholder logic while keeping the original E2E behavior intact.

## Required Internal Modules

- bootstrap
- command_registry
- config
- data_io
- binance_pipeline
- aga_stack
- hft_engine_adapter
- audit
- report
- utils

These must remain modules inside the same physical file.

## Acceptance Criteria

1. One physical Rust file only.
2. Existing CLI commands still work.
3. Help text is cleaner.
4. No runtime unwrap/panic in normal paths.
5. No hardcoded user-specific paths.
6. AGA path still runs E2E.
7. Binance path still runs E2E.
8. Backtest reports still generated.
9. Audit reports still generated.
10. File is materially smaller, cleaner, and easier to maintain.

## Final Instruction

Clean the code aggressively, but preserve behavior. Treat CLI compatibility and E2E reproducibility as hard invariants.
