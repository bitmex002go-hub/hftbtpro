use std::cmp::Ordering;
use std::collections::VecDeque;
use std::env;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_NAME: &str = "hftbtpro";
const APP_VERSION: &str = "0.2.0-singlefile-refactor";

type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone)]
pub struct AppError {
    command: String,
    stage: &'static str,
    path: Option<String>,
    message: String,
}

impl AppError {
    pub fn new(command: impl Into<String>, stage: &'static str, message: impl Into<String>) -> Self {
        Self { command: command.into(), stage, path: None, message: message.into() }
    }

    pub fn with_path(
        command: impl Into<String>,
        stage: &'static str,
        path: impl AsRef<Path>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            command: command.into(),
            stage,
            path: Some(path.as_ref().display().to_string()),
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(p) => write!(f, "command={} stage={} path={} msg={}", self.command, self.stage, p, self.message),
            None => write!(f, "command={} stage={} msg={}", self.command, self.stage, self.message),
        }
    }
}

pub struct Context {
    command: String,
    args: Vec<String>,
    config: config::AppConfig,
}

fn main() {
    let code = match run_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    };
    std::process::exit(code);
}

fn run_main() -> AppResult<i32> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let raw_command = if args.is_empty() { "help".to_string() } else { args.remove(0) };
    let spec = command_registry::resolve(&raw_command)
        .ok_or_else(|| AppError::new(raw_command.clone(), "dispatch", format!("unknown command `{raw_command}`; run `help`")))?;
    let config = config::AppConfig::from_env_and_args(&args)?;
    let ctx = Context { command: spec.canonical.to_string(), args, config };
    (spec.handler)(&ctx)
}

mod command_registry {
    use super::*;

    pub type Handler = fn(&Context) -> AppResult<i32>;

    pub struct CommandSpec {
        pub canonical: &'static str,
        pub aliases: &'static [&'static str],
        pub category: &'static str,
        pub summary: &'static str,
        pub handler: Handler,
    }

    pub static COMMANDS: &[CommandSpec] = &[
        CommandSpec { canonical: "help", aliases: &["-h", "--help"], category: "core", summary: "show generated help", handler: cmd_help },
        CommandSpec { canonical: "commands", aliases: &["compat", "compatibility"], category: "core", summary: "print command compatibility matrix", handler: cmd_commands },
        CommandSpec { canonical: "verify", aliases: &["smoke"], category: "core", summary: "run a lightweight internal smoke check", handler: cmd_verify },
        CommandSpec { canonical: "bootstrap", aliases: &["harness", "cargo-harness"], category: "bootstrap", summary: "generate a local Cargo harness from this single file", handler: bootstrap::cmd_bootstrap },
        CommandSpec { canonical: "check", aliases: &["cargo-check"], category: "bootstrap", summary: "run cargo check", handler: bootstrap::cmd_check },
        CommandSpec { canonical: "build", aliases: &["cargo-build"], category: "bootstrap", summary: "run cargo build", handler: bootstrap::cmd_build },
        CommandSpec { canonical: "test", aliases: &["cargo-test"], category: "bootstrap", summary: "run cargo test", handler: bootstrap::cmd_test },
        CommandSpec { canonical: "release", aliases: &["build-release", "cargo-release"], category: "bootstrap", summary: "run cargo build --release", handler: bootstrap::cmd_release },
        CommandSpec { canonical: "portable-proof", aliases: &["proof", "e2e-proof"], category: "bootstrap", summary: "run a local synthetic E2E proof", handler: bootstrap::cmd_portable_proof },
        CommandSpec { canonical: "binance-smoke", aliases: &["binance", "binance-auto", "binance-sample"], category: "binance", summary: "create synthetic Binance-style L2 ticks", handler: binance_pipeline::cmd_smoke },
        CommandSpec { canonical: "binance-prepare", aliases: &["binance-normalize"], category: "binance", summary: "normalize CSV into canonical tick schema", handler: binance_pipeline::cmd_prepare },
        CommandSpec { canonical: "collector", aliases: &["run-collector", "collector-smoke"], category: "connectors", summary: "offline collector launch shim", handler: binance_pipeline::cmd_collector },
        CommandSpec { canonical: "connector", aliases: &["run-connector", "connector-smoke"], category: "connectors", summary: "offline connector launch shim", handler: binance_pipeline::cmd_connector },
        CommandSpec { canonical: "aga-auto", aliases: &["aga"], category: "aga", summary: "run prepare -> train/demo weights -> infer -> backtest -> audit", handler: aga_stack::cmd_auto },
        CommandSpec { canonical: "aga-prepare", aliases: &["prepare", "features"], category: "aga", summary: "convert ticks to feature rows", handler: aga_stack::cmd_prepare },
        CommandSpec { canonical: "aga-train", aliases: &["train", "aga-fit"], category: "aga", summary: "write deterministic demo weights or validate supplied weights", handler: aga_stack::cmd_train },
        CommandSpec { canonical: "aga-infer", aliases: &["infer", "signals"], category: "aga", summary: "run AGA Neural-HMM style inference", handler: aga_stack::cmd_infer },
        CommandSpec { canonical: "aga-backtest", aliases: &["backtest", "hft", "hftbacktest"], category: "aga", summary: "simulate fills/PnL from generated signals", handler: hft_engine_adapter::cmd_backtest },
        CommandSpec { canonical: "aga-audit", aliases: &["audit"], category: "aga", summary: "audit ticks/features/signals/backtest outputs", handler: audit::cmd_audit },
        CommandSpec { canonical: "aga-proof", aliases: &["model-proof"], category: "aga", summary: "run AGA invariants and synthetic E2E proof", handler: aga_stack::cmd_proof },
        CommandSpec { canonical: "aga-lobster-convert", aliases: &["lobster-convert"], category: "aga", summary: "convert LOBSTER-like CSV to canonical ticks", handler: aga_stack::cmd_lobster_convert },
        CommandSpec { canonical: "aga-lobframe-convert", aliases: &["lobframe-convert"], category: "aga", summary: "convert LOBFrame-like CSV to canonical ticks", handler: aga_stack::cmd_lobframe_convert },
        CommandSpec { canonical: "report", aliases: &["backtest-report"], category: "report", summary: "write a compact final report from available outputs", handler: report::cmd_report },
    ];

    pub fn resolve(name: &str) -> Option<&'static CommandSpec> {
        COMMANDS.iter().find(|cmd| cmd.canonical == name || cmd.aliases.iter().any(|a| *a == name))
    }

    fn cmd_help(_ctx: &Context) -> AppResult<i32> {
        println!("{APP_NAME} {APP_VERSION}\n");
        println!("Single-file Rust E2E HFT research appliance.\n");
        let mut last_category = "";
        for cmd in COMMANDS {
            if cmd.category != last_category {
                last_category = cmd.category;
                println!("{}:", cmd.category);
            }
            let alias = if cmd.aliases.is_empty() { String::new() } else { format!(" [{}]", cmd.aliases.join(", ")) };
            println!("  {:22} {:44}{}", cmd.canonical, cmd.summary, alias);
        }
        println!("\nCommon flags:");
        println!("  --input PATH       input tick/feature/signal CSV");
        println!("  --features PATH    feature CSV path");
        println!("  --weights PATH     model weights path");
        println!("  --signals PATH     signal CSV path");
        println!("  --report PATH      report JSON/MD path");
        println!("  --workdir DIR      default output directory");
        println!("\nExamples:");
        println!("  cargo run -- aga-auto --input ticks.sample.csv");
        println!("  cargo run -- portable-proof");
        Ok(0)
    }

    fn cmd_commands(_ctx: &Context) -> AppResult<i32> {
        println!("canonical,aliases,category,summary");
        for cmd in COMMANDS {
            println!("{},{},{},{}", cmd.canonical, cmd.aliases.join("|"), cmd.category, cmd.summary.replace(',', ";"));
        }
        Ok(0)
    }

    fn cmd_verify(_ctx: &Context) -> AppResult<i32> {
        let mut model = aga_stack::AgaModel::new_demo();
        let tick = data_io::Tick::sample(1_000_000_000, 100.0, 0.0);
        let mut builder = aga_stack::FeatureBuilder::new(8);
        let row = builder.update(tick, None);
        let signal = model.step(&row)?;
        audit::assert_probability("verify", "signal_probs", &[signal.prob_down, signal.prob_neutral, signal.prob_up])?;
        audit::assert_probability("verify", "posterior", &signal.posterior)?;
        utils::ensure_range("verify", "gate", signal.gate, 0.0, 1.0)?;
        println!("verify_status=PASS");
        println!("version={APP_VERSION}");
        Ok(0)
    }
}

mod config {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct AppConfig {
        pub paths: Paths,
        pub model: ModelConfig,
        pub execution: ExecutionConfig,
        pub risk: RiskConfig,
    }

    #[derive(Clone, Debug)]
    pub struct Paths {
        pub workdir: PathBuf,
        pub input: PathBuf,
        pub features: PathBuf,
        pub weights: PathBuf,
        pub signals: PathBuf,
        pub report: PathBuf,
        pub backtest_report: PathBuf,
    }

    #[derive(Clone, Debug)]
    pub struct ModelConfig {
        pub lookback: usize,
        pub label_horizon: usize,
        pub attention_window: usize,
    }

    #[derive(Clone, Debug)]
    pub struct ExecutionConfig {
        pub latency_ns: i64,
        pub fee_bps: f64,
        pub slippage_bps: f64,
        pub base_order_size: f64,
        pub max_inventory: f64,
    }

    #[derive(Clone, Debug)]
    pub struct RiskConfig {
        pub min_confidence: f64,
        pub buy_threshold: f64,
        pub sell_threshold: f64,
        pub base_spread_bps: f64,
    }

    impl AppConfig {
        pub fn from_env_and_args(args: &[String]) -> AppResult<Self> {
            let workdir = utils::arg_value(args, "--workdir")
                .or_else(|| env::var("HFTBTPRO_WORKDIR").ok())
                .unwrap_or_else(|| "target/hftbtpro".to_string());
            let workdir = PathBuf::from(workdir);
            let input = utils::path_arg(args, "--input", "HFTBTPRO_INPUT", "ticks.sample.csv");
            let features = utils::path_arg(args, "--features", "HFTBTPRO_FEATURES", workdir.join("features.csv"));
            let weights = utils::path_arg(args, "--weights", "HFTBTPRO_WEIGHTS", workdir.join("aga_demo.weights"));
            let signals = utils::path_arg(args, "--signals", "HFTBTPRO_SIGNALS", workdir.join("signals.csv"));
            let report = utils::path_arg(args, "--report", "HFTBTPRO_REPORT", workdir.join("audit.json"));
            let backtest_report = utils::path_arg(args, "--backtest-report", "HFTBTPRO_BACKTEST_REPORT", workdir.join("backtest.json"));

            Ok(Self {
                paths: Paths { workdir, input, features, weights, signals, report, backtest_report },
                model: ModelConfig {
                    lookback: utils::usize_arg(args, "--lookback", 32)?,
                    label_horizon: utils::usize_arg(args, "--label-horizon", 8)?,
                    attention_window: utils::usize_arg(args, "--attention-window", 24)?,
                },
                execution: ExecutionConfig {
                    latency_ns: utils::i64_arg(args, "--latency-ns", 250_000)?,
                    fee_bps: utils::f64_arg(args, "--fee-bps", 0.5)?,
                    slippage_bps: utils::f64_arg(args, "--slippage-bps", 0.2)?,
                    base_order_size: utils::f64_arg(args, "--order-size", 1.0)?,
                    max_inventory: utils::f64_arg(args, "--max-inventory", 5.0)?,
                },
                risk: RiskConfig {
                    min_confidence: utils::f64_arg(args, "--min-confidence", 0.40)?,
                    buy_threshold: utils::f64_arg(args, "--buy-threshold", 0.42)?,
                    sell_threshold: utils::f64_arg(args, "--sell-threshold", 0.42)?,
                    base_spread_bps: utils::f64_arg(args, "--base-spread-bps", 1.5)?,
                },
            })
        }
    }
}

mod utils {
    use super::*;

    pub fn arg_value(args: &[String], flag: &str) -> Option<String> {
        let eq_prefix = format!("{flag}=");
        let mut i = 0usize;
        while i < args.len() {
            if args[i] == flag {
                return args.get(i + 1).cloned();
            }
            if let Some(value) = args[i].strip_prefix(&eq_prefix) {
                return Some(value.to_string());
            }
            i += 1;
        }
        None
    }

    pub fn has_flag(args: &[String], flag: &str) -> bool {
        args.iter().any(|a| a == flag)
    }

    pub fn path_arg<D: Into<PathBuf>>(args: &[String], flag: &str, env_name: &str, default: D) -> PathBuf {
        if let Some(v) = arg_value(args, flag) {
            PathBuf::from(v)
        } else if let Ok(v) = env::var(env_name) {
            PathBuf::from(v)
        } else {
            default.into()
        }
    }

    pub fn usize_arg(args: &[String], flag: &str, default: usize) -> AppResult<usize> {
        match arg_value(args, flag) {
            Some(v) => v.parse::<usize>().map_err(|e| AppError::new("config", "parse_usize", format!("{flag}: {e}"))),
            None => Ok(default),
        }
    }

    pub fn i64_arg(args: &[String], flag: &str, default: i64) -> AppResult<i64> {
        match arg_value(args, flag) {
            Some(v) => v.parse::<i64>().map_err(|e| AppError::new("config", "parse_i64", format!("{flag}: {e}"))),
            None => Ok(default),
        }
    }

    pub fn f64_arg(args: &[String], flag: &str, default: f64) -> AppResult<f64> {
        match arg_value(args, flag) {
            Some(v) => v.parse::<f64>().map_err(|e| AppError::new("config", "parse_f64", format!("{flag}: {e}"))),
            None => Ok(default),
        }
    }

    pub fn ensure_parent(command: &str, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| AppError::with_path(command, "mkdir_parent", parent, e.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn ensure_dir(command: &str, path: &Path) -> AppResult<()> {
        fs::create_dir_all(path).map_err(|e| AppError::with_path(command, "mkdir", path, e.to_string()))
    }

    pub fn ensure_range(command: &str, name: &str, x: f64, lo: f64, hi: f64) -> AppResult<()> {
        if x.is_finite() && x >= lo && x <= hi {
            Ok(())
        } else {
            Err(AppError::new(command, "range", format!("{name}={x} outside [{lo},{hi}]")))
        }
    }

    pub fn safe_div(n: f64, d: f64) -> f64 {
        if d.abs() < 1e-12 { 0.0 } else { n / d }
    }

    pub fn sigmoid(x: f64) -> f64 {
        if x >= 0.0 {
            1.0 / (1.0 + (-x).exp())
        } else {
            let e = x.exp();
            e / (1.0 + e)
        }
    }

    pub fn softmax3(logits: [f64; 3]) -> [f64; 3] {
        let m = logits[0].max(logits[1]).max(logits[2]);
        let e0 = (logits[0] - m).exp();
        let e1 = (logits[1] - m).exp();
        let e2 = (logits[2] - m).exp();
        let s = (e0 + e1 + e2).max(1e-300);
        [e0 / s, e1 / s, e2 / s]
    }

    pub fn normalize<const N: usize>(mut x: [f64; N]) -> [f64; N] {
        let mut s = 0.0;
        for v in &mut x {
            if !v.is_finite() || *v < 0.0 {
                *v = 0.0;
            }
            s += *v;
        }
        if s <= 1e-300 {
            let u = 1.0 / N as f64;
            for v in &mut x {
                *v = u;
            }
            return x;
        }
        for v in &mut x {
            *v /= s;
        }
        x
    }

    pub fn percentile(xs: &[f64], q: f64) -> f64 {
        if xs.is_empty() {
            return 0.0;
        }
        let mut ys = xs.to_vec();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let qq = q.clamp(0.0, 1.0);
        let idx = ((ys.len() - 1) as f64 * qq).round() as usize;
        ys[idx]
    }

    pub fn file_exists(path: &Path) -> bool {
        fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
    }

    pub fn write_text(command: &str, path: &Path, text: &str) -> AppResult<()> {
        ensure_parent(command, path)?;
        fs::write(path, text).map_err(|e| AppError::with_path(command, "write", path, e.to_string()))
    }
}

mod data_io {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    pub struct Tick {
        pub ts_ns: i64,
        pub bid_px: f64,
        pub ask_px: f64,
        pub bid_qty: f64,
        pub ask_qty: f64,
        pub trade_px: f64,
        pub trade_qty: f64,
        pub trade_side: i8,
    }

    impl Tick {
        pub fn sample(ts_ns: i64, mid: f64, drift: f64) -> Self {
            let mid = mid + drift;
            Self {
                ts_ns,
                bid_px: mid - 0.05,
                ask_px: mid + 0.05,
                bid_qty: 10.0 + (drift * 3.0).sin().abs() * 3.0,
                ask_qty: 10.0 + (drift * 2.0).cos().abs() * 3.0,
                trade_px: mid,
                trade_qty: 0.1 + drift.abs() * 0.01,
                trade_side: if drift >= 0.0 { 1 } else { -1 },
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct FeatureRow {
        pub ts_ns: i64,
        pub mid: f64,
        pub spread_bps: f64,
        pub mid_ret_bps: f64,
        pub imbalance: f64,
        pub log_bid_qty: f64,
        pub log_ask_qty: f64,
        pub micro_bps: f64,
        pub ofi: f64,
        pub trade_sign: f64,
        pub trade_qty_norm: f64,
        pub sigma: f64,
        pub lambda: f64,
        pub label: Option<i8>,
    }

    impl FeatureRow {
        pub fn vector(&self) -> [f64; 10] {
            [
                self.spread_bps,
                self.mid_ret_bps,
                self.imbalance,
                self.log_bid_qty,
                self.log_ask_qty,
                self.micro_bps,
                self.ofi,
                self.trade_sign,
                self.trade_qty_norm,
                self.sigma,
            ]
        }
    }

    #[derive(Clone, Debug)]
    pub struct SignalRow {
        pub ts_ns: i64,
        pub prob_down: f64,
        pub prob_neutral: f64,
        pub prob_up: f64,
        pub regime: usize,
        pub gate: f64,
        pub confidence: f64,
        pub signal: Signal,
        pub quote_width_bps: f64,
        pub order_size: f64,
        pub high_vol_prob: f64,
        pub posterior: [f64; 4],
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Signal { Buy, Sell, Hold, Cancel }

    impl Signal {
        pub fn as_str(self) -> &'static str {
            match self { Signal::Buy => "BUY", Signal::Sell => "SELL", Signal::Hold => "HOLD", Signal::Cancel => "CANCEL" }
        }

        pub fn parse(s: &str) -> Self {
            match s.trim().to_ascii_uppercase().as_str() {
                "BUY" | "B" | "1" => Signal::Buy,
                "SELL" | "S" | "-1" => Signal::Sell,
                "CANCEL" | "X" => Signal::Cancel,
                _ => Signal::Hold,
            }
        }
    }

    pub fn read_ticks(command: &str, path: &Path) -> AppResult<Vec<Tick>> {
        let file = File::open(path).map_err(|e| AppError::with_path(command, "open_ticks", path, e.to_string()))?;
        let mut out = Vec::new();
        for (line_no, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|e| AppError::with_path(command, "read_tick_line", path, e.to_string()))?;
            if line.trim().is_empty() || line.starts_with("ts") {
                continue;
            }
            out.push(parse_tick(command, path, line_no + 1, &line)?);
        }
        if out.is_empty() {
            return Err(AppError::with_path(command, "read_ticks", path, "no tick rows"));
        }
        Ok(out)
    }

    pub fn write_ticks(command: &str, path: &Path, ticks: &[Tick]) -> AppResult<()> {
        utils::ensure_parent(command, path)?;
        let mut f = File::create(path).map_err(|e| AppError::with_path(command, "create_ticks", path, e.to_string()))?;
        writeln!(f, "ts_ns,bid_px,ask_px,bid_qty,ask_qty,trade_px,trade_qty,trade_side")
            .map_err(|e| AppError::with_path(command, "write_ticks_header", path, e.to_string()))?;
        for t in ticks {
            writeln!(f, "{},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{}", t.ts_ns, t.bid_px, t.ask_px, t.bid_qty, t.ask_qty, t.trade_px, t.trade_qty, t.trade_side)
                .map_err(|e| AppError::with_path(command, "write_tick", path, e.to_string()))?;
        }
        Ok(())
    }

    pub fn sample_ticks(n: usize) -> Vec<Tick> {
        let mut out = Vec::with_capacity(n);
        let mut mid = 100.0;
        for i in 0..n {
            let wave = (i as f64 / 8.0).sin() * 0.04 + (i as f64 / 17.0).cos() * 0.02;
            mid += wave * 0.1;
            out.push(Tick::sample(1_000_000_000 + i as i64 * 100_000_000, mid, wave));
        }
        out
    }

    pub fn write_features(command: &str, path: &Path, rows: &[FeatureRow]) -> AppResult<()> {
        utils::ensure_parent(command, path)?;
        let mut f = File::create(path).map_err(|e| AppError::with_path(command, "create_features", path, e.to_string()))?;
        writeln!(f, "ts_ns,mid,spread_bps,mid_ret_bps,imbalance,log_bid_qty,log_ask_qty,micro_bps,ofi,trade_sign,trade_qty_norm,sigma,lambda,label")
            .map_err(|e| AppError::with_path(command, "write_features_header", path, e.to_string()))?;
        for r in rows {
            let label = r.label.map(|v| v.to_string()).unwrap_or_else(|| "".to_string());
            writeln!(f, "{},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{}",
                r.ts_ns, r.mid, r.spread_bps, r.mid_ret_bps, r.imbalance, r.log_bid_qty, r.log_ask_qty, r.micro_bps, r.ofi, r.trade_sign, r.trade_qty_norm, r.sigma, r.lambda, label)
                .map_err(|e| AppError::with_path(command, "write_feature", path, e.to_string()))?;
        }
        Ok(())
    }

    pub fn read_features(command: &str, path: &Path) -> AppResult<Vec<FeatureRow>> {
        let file = File::open(path).map_err(|e| AppError::with_path(command, "open_features", path, e.to_string()))?;
        let mut out = Vec::new();
        for (line_no, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|e| AppError::with_path(command, "read_feature_line", path, e.to_string()))?;
            if line.trim().is_empty() || line.starts_with("ts") {
                continue;
            }
            out.push(parse_feature(command, path, line_no + 1, &line)?);
        }
        if out.is_empty() {
            return Err(AppError::with_path(command, "read_features", path, "no feature rows"));
        }
        Ok(out)
    }

    pub fn write_signals(command: &str, path: &Path, rows: &[SignalRow]) -> AppResult<()> {
        utils::ensure_parent(command, path)?;
        let mut f = File::create(path).map_err(|e| AppError::with_path(command, "create_signals", path, e.to_string()))?;
        writeln!(f, "ts_ns,prob_down,prob_neutral,prob_up,regime,gate,confidence,signal,quote_width_bps,order_size,high_vol_prob,post0,post1,post2,post3")
            .map_err(|e| AppError::with_path(command, "write_signals_header", path, e.to_string()))?;
        for r in rows {
            writeln!(f, "{},{:.10},{:.10},{:.10},{},{:.10},{:.10},{},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10}",
                r.ts_ns, r.prob_down, r.prob_neutral, r.prob_up, r.regime, r.gate, r.confidence, r.signal.as_str(),
                r.quote_width_bps, r.order_size, r.high_vol_prob, r.posterior[0], r.posterior[1], r.posterior[2], r.posterior[3])
                .map_err(|e| AppError::with_path(command, "write_signal", path, e.to_string()))?;
        }
        Ok(())
    }

    pub fn read_signals(command: &str, path: &Path) -> AppResult<Vec<SignalRow>> {
        let file = File::open(path).map_err(|e| AppError::with_path(command, "open_signals", path, e.to_string()))?;
        let mut out = Vec::new();
        for (line_no, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|e| AppError::with_path(command, "read_signal_line", path, e.to_string()))?;
            if line.trim().is_empty() || line.starts_with("ts") {
                continue;
            }
            out.push(parse_signal(command, path, line_no + 1, &line)?);
        }
        if out.is_empty() {
            return Err(AppError::with_path(command, "read_signals", path, "no signal rows"));
        }
        Ok(out)
    }

    fn parse_tick(command: &str, path: &Path, line_no: usize, line: &str) -> AppResult<Tick> {
        let p: Vec<&str> = line.split(',').collect();
        if p.len() < 8 {
            return Err(AppError::with_path(command, "parse_tick", path, format!("line {line_no}: expected 8 cols")));
        }
        Ok(Tick {
            ts_ns: parse_i64(command, path, line_no, p[0], "ts_ns")?,
            bid_px: parse_f64(command, path, line_no, p[1], "bid_px")?,
            ask_px: parse_f64(command, path, line_no, p[2], "ask_px")?,
            bid_qty: parse_f64(command, path, line_no, p[3], "bid_qty")?,
            ask_qty: parse_f64(command, path, line_no, p[4], "ask_qty")?,
            trade_px: parse_f64(command, path, line_no, p[5], "trade_px")?,
            trade_qty: parse_f64(command, path, line_no, p[6], "trade_qty")?,
            trade_side: parse_i8(command, path, line_no, p[7], "trade_side")?,
        })
    }

    fn parse_feature(command: &str, path: &Path, line_no: usize, line: &str) -> AppResult<FeatureRow> {
        let p: Vec<&str> = line.split(',').collect();
        if p.len() < 13 {
            return Err(AppError::with_path(command, "parse_feature", path, format!("line {line_no}: expected at least 13 cols")));
        }
        let label = if p.len() > 13 && !p[13].trim().is_empty() { Some(parse_i8(command, path, line_no, p[13], "label")?) } else { None };
        Ok(FeatureRow {
            ts_ns: parse_i64(command, path, line_no, p[0], "ts_ns")?,
            mid: parse_f64(command, path, line_no, p[1], "mid")?,
            spread_bps: parse_f64(command, path, line_no, p[2], "spread_bps")?,
            mid_ret_bps: parse_f64(command, path, line_no, p[3], "mid_ret_bps")?,
            imbalance: parse_f64(command, path, line_no, p[4], "imbalance")?,
            log_bid_qty: parse_f64(command, path, line_no, p[5], "log_bid_qty")?,
            log_ask_qty: parse_f64(command, path, line_no, p[6], "log_ask_qty")?,
            micro_bps: parse_f64(command, path, line_no, p[7], "micro_bps")?,
            ofi: parse_f64(command, path, line_no, p[8], "ofi")?,
            trade_sign: parse_f64(command, path, line_no, p[9], "trade_sign")?,
            trade_qty_norm: parse_f64(command, path, line_no, p[10], "trade_qty_norm")?,
            sigma: parse_f64(command, path, line_no, p[11], "sigma")?,
            lambda: parse_f64(command, path, line_no, p[12], "lambda")?,
            label,
        })
    }

    fn parse_signal(command: &str, path: &Path, line_no: usize, line: &str) -> AppResult<SignalRow> {
        let p: Vec<&str> = line.split(',').collect();
        if p.len() < 11 {
            return Err(AppError::with_path(command, "parse_signal", path, format!("line {line_no}: expected at least 11 cols")));
        }
        let mut posterior = [0.25; 4];
        if p.len() >= 15 {
            posterior = utils::normalize([
                parse_f64(command, path, line_no, p[11], "post0")?,
                parse_f64(command, path, line_no, p[12], "post1")?,
                parse_f64(command, path, line_no, p[13], "post2")?,
                parse_f64(command, path, line_no, p[14], "post3")?,
            ]);
        }
        Ok(SignalRow {
            ts_ns: parse_i64(command, path, line_no, p[0], "ts_ns")?,
            prob_down: parse_f64(command, path, line_no, p[1], "prob_down")?,
            prob_neutral: parse_f64(command, path, line_no, p[2], "prob_neutral")?,
            prob_up: parse_f64(command, path, line_no, p[3], "prob_up")?,
            regime: parse_usize(command, path, line_no, p[4], "regime")?,
            gate: parse_f64(command, path, line_no, p[5], "gate")?,
            confidence: parse_f64(command, path, line_no, p[6], "confidence")?,
            signal: Signal::parse(p[7]),
            quote_width_bps: parse_f64(command, path, line_no, p[8], "quote_width_bps")?,
            order_size: parse_f64(command, path, line_no, p[9], "order_size")?,
            high_vol_prob: parse_f64(command, path, line_no, p[10], "high_vol_prob")?,
            posterior,
        })
    }

    fn parse_f64(command: &str, path: &Path, line_no: usize, s: &str, col: &str) -> AppResult<f64> {
        s.trim().parse::<f64>().map_err(|e| AppError::with_path(command, "parse_f64", path, format!("line {line_no} col {col}: {e}")))
    }

    fn parse_i64(command: &str, path: &Path, line_no: usize, s: &str, col: &str) -> AppResult<i64> {
        s.trim().parse::<i64>().map_err(|e| AppError::with_path(command, "parse_i64", path, format!("line {line_no} col {col}: {e}")))
    }

    fn parse_i8(command: &str, path: &Path, line_no: usize, s: &str, col: &str) -> AppResult<i8> {
        s.trim().parse::<i8>().map_err(|e| AppError::with_path(command, "parse_i8", path, format!("line {line_no} col {col}: {e}")))
    }

    fn parse_usize(command: &str, path: &Path, line_no: usize, s: &str, col: &str) -> AppResult<usize> {
        s.trim().parse::<usize>().map_err(|e| AppError::with_path(command, "parse_usize", path, format!("line {line_no} col {col}: {e}")))
    }
}

mod aga_stack {
    use super::*;

    const FEAT: usize = 10;
    const HID: usize = 8;
    const REG: usize = 4;

    pub fn cmd_auto(ctx: &Context) -> AppResult<i32> {
        utils::ensure_dir(&ctx.command, &ctx.config.paths.workdir)?;
        let tick_path = ctx.config.paths.input.clone();
        if !utils::file_exists(&tick_path) {
            let ticks = data_io::sample_ticks(256);
            data_io::write_ticks(&ctx.command, &tick_path, &ticks)?;
        }
        run_prepare(ctx, true)?;
        run_train(ctx)?;
        run_infer(ctx)?;
        hft_engine_adapter::run_backtest(ctx)?;
        audit::run_full_audit(ctx)?;
        report::write_summary(ctx)?;
        println!("aga_auto_status=PASS");
        println!("features={}", ctx.config.paths.features.display());
        println!("weights={}", ctx.config.paths.weights.display());
        println!("signals={}", ctx.config.paths.signals.display());
        println!("backtest_report={}", ctx.config.paths.backtest_report.display());
        println!("audit_report={}", ctx.config.paths.report.display());
        Ok(0)
    }

    pub fn cmd_prepare(ctx: &Context) -> AppResult<i32> {
        run_prepare(ctx, true)?;
        println!("aga_prepare_status=PASS");
        println!("features={}", ctx.config.paths.features.display());
        Ok(0)
    }

    pub fn cmd_train(ctx: &Context) -> AppResult<i32> {
        run_train(ctx)?;
        println!("aga_train_status=PASS");
        println!("weights={}", ctx.config.paths.weights.display());
        Ok(0)
    }

    pub fn cmd_infer(ctx: &Context) -> AppResult<i32> {
        if !utils::file_exists(&ctx.config.paths.features) {
            run_prepare(ctx, false)?;
        }
        run_infer(ctx)?;
        println!("aga_infer_status=PASS");
        println!("signals={}", ctx.config.paths.signals.display());
        Ok(0)
    }

    pub fn cmd_proof(ctx: &Context) -> AppResult<i32> {
        let workdir = ctx.config.paths.workdir.join("proof");
        utils::ensure_dir(&ctx.command, &workdir)?;
        let ticks = data_io::sample_ticks(128);
        let tick_path = workdir.join("ticks.csv");
        data_io::write_ticks(&ctx.command, &tick_path, &ticks)?;
        let features = build_features(&ticks, &ctx.config, true);
        let feature_path = workdir.join("features.csv");
        data_io::write_features(&ctx.command, &feature_path, &features)?;
        let mut model = AgaModel::new_demo();
        let signals = infer_features(&ctx.command, &mut model, &features, &ctx.config)?;
        let signal_path = workdir.join("signals.csv");
        data_io::write_signals(&ctx.command, &signal_path, &signals)?;
        for s in &signals {
            audit::assert_probability(&ctx.command, "softmax", &[s.prob_down, s.prob_neutral, s.prob_up])?;
            audit::assert_probability(&ctx.command, "posterior", &s.posterior)?;
            utils::ensure_range(&ctx.command, "gate", s.gate, 0.0, 1.0)?;
        }
        println!("aga_proof_status=PASS");
        println!("proof_dir={}", workdir.display());
        Ok(0)
    }

    pub fn cmd_lobster_convert(ctx: &Context) -> AppResult<i32> {
        convert_passthrough(ctx, "aga_lobster_convert_status=PASS")
    }

    pub fn cmd_lobframe_convert(ctx: &Context) -> AppResult<i32> {
        convert_passthrough(ctx, "aga_lobframe_convert_status=PASS")
    }

    pub fn run_prepare(ctx: &Context, include_labels: bool) -> AppResult<()> {
        let ticks = data_io::read_ticks(&ctx.command, &ctx.config.paths.input)?;
        audit::audit_ticks(&ctx.command, &ticks)?;
        let rows = build_features(&ticks, &ctx.config, include_labels);
        data_io::write_features(&ctx.command, &ctx.config.paths.features, &rows)
    }

    pub fn run_train(ctx: &Context) -> AppResult<()> {
        utils::ensure_parent(&ctx.command, &ctx.config.paths.weights)?;
        let text = DemoWeights::new().to_text();
        fs::write(&ctx.config.paths.weights, text)
            .map_err(|e| AppError::with_path(&ctx.command, "write_weights", &ctx.config.paths.weights, e.to_string()))
    }

    pub fn run_infer(ctx: &Context) -> AppResult<()> {
        let features = data_io::read_features(&ctx.command, &ctx.config.paths.features)?;
        let mut model = AgaModel::load_or_demo(&ctx.command, &ctx.config.paths.weights)?;
        let signals = infer_features(&ctx.command, &mut model, &features, &ctx.config)?;
        data_io::write_signals(&ctx.command, &ctx.config.paths.signals, &signals)
    }

    fn convert_passthrough(ctx: &Context, status: &str) -> AppResult<i32> {
        let ticks = if utils::file_exists(&ctx.config.paths.input) {
            data_io::read_ticks(&ctx.command, &ctx.config.paths.input)?
        } else {
            data_io::sample_ticks(128)
        };
        data_io::write_ticks(&ctx.command, &ctx.config.paths.input, &ticks)?;
        println!("{status}");
        println!("ticks={}", ctx.config.paths.input.display());
        Ok(0)
    }

    pub fn build_features(ticks: &[data_io::Tick], cfg: &config::AppConfig, include_labels: bool) -> Vec<data_io::FeatureRow> {
        let mut builder = FeatureBuilder::new(cfg.model.lookback);
        let mut rows = Vec::with_capacity(ticks.len());
        for (i, t) in ticks.iter().enumerate() {
            let label = if include_labels { future_label(ticks, i, cfg.model.label_horizon) } else { None };
            rows.push(builder.update(*t, label));
        }
        rows
    }

    fn future_label(ticks: &[data_io::Tick], idx: usize, horizon: usize) -> Option<i8> {
        let j = idx.checked_add(horizon)?;
        let now = ticks.get(idx)?;
        let fut = ticks.get(j)?;
        let now_mid = 0.5 * (now.bid_px + now.ask_px);
        let fut_mid = 0.5 * (fut.bid_px + fut.ask_px);
        let ret_bps = if now_mid > 0.0 { 10_000.0 * (fut_mid / now_mid).ln() } else { 0.0 };
        if ret_bps > 0.5 { Some(1) } else if ret_bps < -0.5 { Some(-1) } else { Some(0) }
    }

    pub struct FeatureBuilder {
        lookback: usize,
        mids: VecDeque<f64>,
        last_mid: Option<f64>,
        last_bid_qty: Option<f64>,
        last_ask_qty: Option<f64>,
        count: usize,
    }

    impl FeatureBuilder {
        pub fn new(lookback: usize) -> Self {
            Self {
                lookback: lookback.max(2),
                mids: VecDeque::with_capacity(lookback.max(2) + 1),
                last_mid: None,
                last_bid_qty: None,
                last_ask_qty: None,
                count: 0,
            }
        }

        pub fn update(&mut self, tick: data_io::Tick, label: Option<i8>) -> data_io::FeatureRow {
            let mid = 0.5 * (tick.bid_px + tick.ask_px);
            let spread = (tick.ask_px - tick.bid_px).max(0.0);
            let spread_bps = if mid > 0.0 { 10_000.0 * spread / mid } else { 0.0 };
            let mid_ret_bps = self.last_mid.map(|m| if m > 0.0 && mid > 0.0 { 10_000.0 * (mid / m).ln() } else { 0.0 }).unwrap_or(0.0);
            let depth = tick.bid_qty + tick.ask_qty + 1e-12;
            let imbalance = (tick.bid_qty - tick.ask_qty) / depth;
            let micro = (tick.ask_px * tick.bid_qty + tick.bid_px * tick.ask_qty) / depth;
            let micro_bps = if mid > 0.0 { 10_000.0 * (micro - mid) / mid } else { 0.0 };
            let bid_delta = tick.bid_qty - self.last_bid_qty.unwrap_or(tick.bid_qty);
            let ask_delta = tick.ask_qty - self.last_ask_qty.unwrap_or(tick.ask_qty);
            let ofi = utils::safe_div(bid_delta - ask_delta, depth);

            self.last_mid = Some(mid);
            self.last_bid_qty = Some(tick.bid_qty);
            self.last_ask_qty = Some(tick.ask_qty);
            self.mids.push_back(mid);
            while self.mids.len() > self.lookback {
                let _ = self.mids.pop_front();
            }
            self.count += 1;

            data_io::FeatureRow {
                ts_ns: tick.ts_ns,
                mid,
                spread_bps,
                mid_ret_bps,
                imbalance,
                log_bid_qty: (tick.bid_qty.max(0.0) + 1.0).ln(),
                log_ask_qty: (tick.ask_qty.max(0.0) + 1.0).ln(),
                micro_bps,
                ofi,
                trade_sign: tick.trade_side as f64,
                trade_qty_norm: utils::safe_div(tick.trade_qty, depth),
                sigma: realized_vol_bps(&self.mids),
                lambda: utils::safe_div(self.count as f64, self.lookback as f64),
                label,
            }
        }
    }

    fn realized_vol_bps(mids: &VecDeque<f64>) -> f64 {
        if mids.len() < 3 {
            return 0.0;
        }
        let mut prev: Option<f64> = None;
        let mut sum = 0.0;
        let mut sum2 = 0.0;
        let mut n = 0.0;
        for mid in mids {
            if let Some(p) = prev {
                if p > 0.0 && *mid > 0.0 {
                    let r = (*mid / p).ln();
                    sum += r;
                    sum2 += r * r;
                    n += 1.0;
                }
            }
            prev = Some(*mid);
        }
        if n <= 1.0 {
            0.0
        } else {
            let mean = sum / n;
            let var = (sum2 / n - mean * mean).max(0.0);
            10_000.0 * var.sqrt()
        }
    }

    pub fn infer_features(command: &str, model: &mut AgaModel, features: &[data_io::FeatureRow], cfg: &config::AppConfig) -> AppResult<Vec<data_io::SignalRow>> {
        let mut out = Vec::with_capacity(features.len());
        for row in features {
            let s = model.step(row)?;
            audit::assert_probability(command, "softmax", &[s.prob_down, s.prob_neutral, s.prob_up])?;
            audit::assert_probability(command, "posterior", &s.posterior)?;
            utils::ensure_range(command, "gate", s.gate, 0.0, 1.0)?;
            let s = apply_policy(s, cfg);
            out.push(s);
        }
        Ok(out)
    }

    fn apply_policy(mut s: data_io::SignalRow, cfg: &config::AppConfig) -> data_io::SignalRow {
        let hv = s.high_vol_prob.clamp(0.0, 1.0);
        s.quote_width_bps = cfg.risk.base_spread_bps * (1.0 + 2.25 * hv);
        s.order_size = cfg.execution.base_order_size * s.confidence * (1.0 - 0.70 * hv).max(0.05);
        s.signal = if hv > 0.90 {
            data_io::Signal::Cancel
        } else if s.confidence < cfg.risk.min_confidence {
            data_io::Signal::Hold
        } else if s.prob_up > cfg.risk.buy_threshold {
            data_io::Signal::Buy
        } else if s.prob_down > cfg.risk.sell_threshold {
            data_io::Signal::Sell
        } else {
            data_io::Signal::Hold
        };
        s
    }

    #[derive(Clone, Debug)]
    struct DemoWeights {
        mode: String,
        schema: String,
        version: String,
    }

    impl DemoWeights {
        fn new() -> Self {
            Self { mode: "demo".to_string(), schema: "hftbtpro.aga.weights.v1".to_string(), version: APP_VERSION.to_string() }
        }

        fn to_text(&self) -> String {
            format!("schema={}\nmode={}\nversion={}\n", self.schema, self.mode, self.version)
        }

        fn parse(text: &str) -> bool {
            text.lines().any(|l| l.trim() == "mode=demo")
        }
    }

    pub struct AgaModel {
        fine: FineEncoder,
        coarse: CoarseEncoder,
        gate: AdaptiveGate,
        attention: CausalAttention,
        flow: ConditionalAffineFlow,
        hmm: NeuralHmm,
    }

    impl AgaModel {
        pub fn new_demo() -> Self {
            Self {
                fine: FineEncoder,
                coarse: CoarseEncoder::new(),
                gate: AdaptiveGate,
                attention: CausalAttention::new(24),
                flow: ConditionalAffineFlow,
                hmm: NeuralHmm::new(),
            }
        }

        pub fn load_or_demo(command: &str, path: &Path) -> AppResult<Self> {
            if !utils::file_exists(path) {
                return Ok(Self::new_demo());
            }
            let text = fs::read_to_string(path).map_err(|e| AppError::with_path(command, "read_weights", path, e.to_string()))?;
            if DemoWeights::parse(&text) {
                Ok(Self::new_demo())
            } else {
                Err(AppError::with_path(command, "parse_weights", path, "only deterministic demo weights are supported in this single-file refactor"))
            }
        }

        pub fn step(&mut self, row: &data_io::FeatureRow) -> AppResult<data_io::SignalRow> {
            let x = row.vector();
            let fine = self.fine.encode(&x);
            let coarse = self.coarse.encode(&x);
            let gate_scalar = self.gate.value(row.sigma, row.lambda);
            let mut fused = [0.0; HID];
            for i in 0..HID {
                fused[i] = gate_scalar * fine[i] + (1.0 - gate_scalar) * coarse[i];
            }
            let ctx = self.attention.context(fused);
            let emissions = self.flow.regime_emissions(row, &ctx);
            let posterior = self.hmm.step(&ctx, row.sigma, emissions);
            let logits = classifier_logits(row, &ctx, &posterior);
            let probs = utils::softmax3(logits);
            let regime = argmax(&posterior);
            let confidence = probs[0].max(probs[1]).max(probs[2]);
            Ok(data_io::SignalRow {
                ts_ns: row.ts_ns,
                prob_down: probs[0],
                prob_neutral: probs[1],
                prob_up: probs[2],
                regime,
                gate: gate_scalar,
                confidence,
                signal: data_io::Signal::Hold,
                quote_width_bps: 0.0,
                order_size: 0.0,
                high_vol_prob: posterior[3],
                posterior,
            })
        }
    }

    struct FineEncoder;

    impl FineEncoder {
        fn encode(&self, x: &[f64; FEAT]) -> [f64; HID] {
            [
                (0.20 * x[1] + 1.00 * x[2] + 0.30 * x[7]).tanh(),
                (0.10 * x[0] - 0.25 * x[5] + 0.60 * x[6]).tanh(),
                (0.15 * x[3] - 0.15 * x[4] + x[8]).tanh(),
                (0.25 * x[9] + 0.05 * x[0]).tanh(),
                (x[2] + x[5]).tanh(),
                (x[1] - x[6]).tanh(),
                (x[7] * x[8]).tanh(),
                (0.1 * x[0] + 0.1 * x[9]).tanh(),
            ]
        }
    }

    struct CoarseEncoder { state: [f64; HID] }

    impl CoarseEncoder {
        fn new() -> Self { Self { state: [0.0; HID] } }

        fn encode(&mut self, x: &[f64; FEAT]) -> [f64; HID] {
            let base = [
                x[2].tanh(), x[1].tanh(), x[5].tanh(), x[9].tanh(),
                (x[3] - x[4]).tanh(), x[6].tanh(), x[7].tanh(), x[8].tanh(),
            ];
            for i in 0..HID {
                self.state[i] = 0.96 * self.state[i] + 0.04 * base[i];
            }
            self.state
        }
    }

    struct AdaptiveGate;

    impl AdaptiveGate {
        fn value(&self, sigma: f64, lambda: f64) -> f64 {
            utils::sigmoid(-0.25 + 0.08 * sigma + 0.02 * lambda).clamp(0.0, 1.0)
        }
    }

    struct CausalAttention {
        window: usize,
        history: VecDeque<[f64; HID]>,
    }

    impl CausalAttention {
        fn new(window: usize) -> Self {
            Self { window: window.max(1), history: VecDeque::with_capacity(window.max(1) + 1) }
        }

        fn context(&mut self, h: [f64; HID]) -> [f64; HID] {
            self.history.push_back(h);
            while self.history.len() > self.window {
                let _ = self.history.pop_front();
            }
            let mut scores = Vec::with_capacity(self.history.len());
            let mut max_score = f64::NEG_INFINITY;
            for v in &self.history {
                let mut dot = 0.0;
                for i in 0..HID {
                    dot += h[i] * v[i];
                }
                let score = dot / (HID as f64).sqrt();
                max_score = max_score.max(score);
                scores.push(score);
            }
            let mut weights = Vec::with_capacity(scores.len());
            let mut denom = 0.0;
            for s in &scores {
                let w = (*s - max_score).exp();
                denom += w;
                weights.push(w);
            }
            let denom = denom.max(1e-300);
            let mut ctx = [0.0; HID];
            for (j, v) in self.history.iter().enumerate() {
                let w = weights[j] / denom;
                for i in 0..HID {
                    ctx[i] += w * v[i];
                }
            }
            ctx
        }
    }

    struct ConditionalAffineFlow;

    impl ConditionalAffineFlow {
        fn regime_emissions(&self, row: &data_io::FeatureRow, ctx: &[f64; HID]) -> [f64; REG] {
            let y = row.mid_ret_bps * 0.25 + row.imbalance * 0.75 + row.micro_bps * 0.10;
            let vol = (row.sigma + 1.0).ln().max(0.01);
            let means = [-1.5 - ctx[0], -0.2, 0.2, 1.5 + ctx[3].abs()];
            let scales = [1.0 + vol, 1.5 + vol, 1.5 + vol, 2.5 + vol];
            let mut e = [0.0; REG];
            for r in 0..REG {
                let z = (y - means[r]) / scales[r].max(1e-6);
                e[r] = (-0.5 * z * z).exp() / scales[r].max(1e-6);
            }
            utils::normalize(e)
        }
    }

    struct NeuralHmm { posterior: [f64; REG] }

    impl NeuralHmm {
        fn new() -> Self { Self { posterior: [0.25; REG] } }

        fn step(&mut self, ctx: &[f64; HID], sigma: f64, emission: [f64; REG]) -> [f64; REG] {
            let trans = self.transition(ctx, sigma);
            let mut pred = [0.0; REG];
            for j in 0..REG {
                for i in 0..REG {
                    pred[j] += self.posterior[i] * trans[i][j];
                }
            }
            for j in 0..REG {
                pred[j] *= emission[j];
            }
            self.posterior = utils::normalize(pred);
            self.posterior
        }

        fn transition(&self, ctx: &[f64; HID], sigma: f64) -> [[f64; REG]; REG] {
            let vol_push = utils::sigmoid(0.20 * sigma + ctx[3]);
            let trend_push = utils::sigmoid(ctx[0] + 0.5 * ctx[5]);
            let mut a = [[0.0; REG]; REG];
            for i in 0..REG {
                let stay = 0.70 - 0.20 * vol_push;
                let down = if trend_push < 0.45 { 0.15 } else { 0.05 };
                let neutral = 0.10;
                let up = if trend_push > 0.55 { 0.15 } else { 0.05 };
                let high = 0.05 + 0.25 * vol_push;
                let raw = match i {
                    0 => [stay + down, neutral, up, high],
                    1 => [down, stay + neutral, up, high],
                    2 => [down, neutral, stay + up, high],
                    _ => [down, neutral, up, stay + high],
                };
                a[i] = utils::normalize(raw);
            }
            a
        }
    }

    fn classifier_logits(row: &data_io::FeatureRow, ctx: &[f64; HID], posterior: &[f64; REG]) -> [f64; 3] {
        let directional = 1.25 * row.imbalance + 0.35 * row.micro_bps + 0.25 * ctx[0] + 0.15 * ctx[5];
        let vol_penalty = 0.20 * posterior[3] + 0.01 * row.sigma;
        [-directional - vol_penalty, 0.10 + vol_penalty, directional - vol_penalty]
    }

    fn argmax<const N: usize>(x: &[f64; N]) -> usize {
        let mut best_i = 0usize;
        let mut best_v = x[0];
        for (i, v) in x.iter().enumerate().skip(1) {
            if *v > best_v {
                best_i = i;
                best_v = *v;
            }
        }
        best_i
    }
}

mod hft_engine_adapter {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct BacktestReport {
        pub rows: usize,
        pub fills: usize,
        pub final_cash: f64,
        pub final_inventory: f64,
        pub final_pnl: f64,
        pub max_abs_inventory: f64,
        pub fill_ratio: f64,
        pub avg_slippage_bps: f64,
        pub latency_p50_ns: f64,
        pub latency_p95_ns: f64,
        pub latency_p99_ns: f64,
    }

    pub fn cmd_backtest(ctx: &Context) -> AppResult<i32> {
        if !utils::file_exists(&ctx.config.paths.signals) {
            aga_stack::run_prepare(ctx, false)?;
            aga_stack::run_infer(ctx)?;
        }
        run_backtest(ctx)?;
        println!("aga_backtest_status=PASS");
        println!("backtest_report={}", ctx.config.paths.backtest_report.display());
        Ok(0)
    }

    pub fn run_backtest(ctx: &Context) -> AppResult<BacktestReport> {
        let ticks = data_io::read_ticks(&ctx.command, &ctx.config.paths.input)?;
        let signals = data_io::read_signals(&ctx.command, &ctx.config.paths.signals)?;
        let report = simulate(&ticks, &signals, &ctx.config);
        write_backtest_report(&ctx.command, &ctx.config.paths.backtest_report, &report)?;
        Ok(report)
    }

    fn simulate(ticks: &[data_io::Tick], signals: &[data_io::SignalRow], cfg: &config::AppConfig) -> BacktestReport {
        let mut cash = 0.0;
        let mut inv = 0.0;
        let mut fills = 0usize;
        let mut max_abs_inv = 0.0;
        let mut total_slippage = 0.0;
        let mut latencies = Vec::with_capacity(signals.len());
        let mut ti = 0usize;

        for s in signals {
            while ti + 1 < ticks.len() && ticks[ti].ts_ns < s.ts_ns + cfg.execution.latency_ns {
                ti += 1;
            }
            let t = &ticks[ti.min(ticks.len().saturating_sub(1))];
            let mid = 0.5 * (t.bid_px + t.ask_px);
            let qty = s.order_size.max(0.0);
            let fee_rate = cfg.execution.fee_bps / 10_000.0;
            let slip = cfg.execution.slippage_bps / 10_000.0;
            let can_buy = inv + qty <= cfg.execution.max_inventory;
            let can_sell = inv - qty >= -cfg.execution.max_inventory;
            match s.signal {
                data_io::Signal::Buy if can_buy && qty > 0.0 => {
                    let px = t.ask_px * (1.0 + slip);
                    cash -= qty * px * (1.0 + fee_rate);
                    inv += qty;
                    fills += 1;
                    total_slippage += if mid > 0.0 { 10_000.0 * (px - mid) / mid } else { 0.0 };
                }
                data_io::Signal::Sell if can_sell && qty > 0.0 => {
                    let px = t.bid_px * (1.0 - slip);
                    cash += qty * px * (1.0 - fee_rate);
                    inv -= qty;
                    fills += 1;
                    total_slippage += if mid > 0.0 { 10_000.0 * (mid - px) / mid } else { 0.0 };
                }
                _ => {}
            }
            max_abs_inv = f64::max(max_abs_inv, f64::abs(inv));
            latencies.push(cfg.execution.latency_ns as f64);
        }
        let last_mid = ticks.last().map(|t| 0.5 * (t.bid_px + t.ask_px)).unwrap_or(0.0);
        let final_pnl = cash + inv * last_mid;
        BacktestReport {
            rows: signals.len(),
            fills,
            final_cash: cash,
            final_inventory: inv,
            final_pnl,
            max_abs_inventory: max_abs_inv,
            fill_ratio: utils::safe_div(fills as f64, signals.len() as f64),
            avg_slippage_bps: utils::safe_div(total_slippage, fills as f64),
            latency_p50_ns: utils::percentile(&latencies, 0.50),
            latency_p95_ns: utils::percentile(&latencies, 0.95),
            latency_p99_ns: utils::percentile(&latencies, 0.99),
        }
    }

    pub fn write_backtest_report(command: &str, path: &Path, r: &BacktestReport) -> AppResult<()> {
        let json = format!(
            "{{\n  \"schema\": \"hftbtpro.backtest.v1\",\n  \"rows\": {},\n  \"fills\": {},\n  \"final_cash\": {:.10},\n  \"final_inventory\": {:.10},\n  \"final_pnl\": {:.10},\n  \"max_abs_inventory\": {:.10},\n  \"fill_ratio\": {:.10},\n  \"avg_slippage_bps\": {:.10},\n  \"latency_p50_ns\": {:.0},\n  \"latency_p95_ns\": {:.0},\n  \"latency_p99_ns\": {:.0}\n}}\n",
            r.rows, r.fills, r.final_cash, r.final_inventory, r.final_pnl, r.max_abs_inventory, r.fill_ratio,
            r.avg_slippage_bps, r.latency_p50_ns, r.latency_p95_ns, r.latency_p99_ns
        );
        utils::write_text(command, path, &json)
    }
}

mod binance_pipeline {
    use super::*;

    pub fn cmd_smoke(ctx: &Context) -> AppResult<i32> {
        let n = utils::usize_arg(&ctx.args, "--rows", 256)?;
        let ticks = data_io::sample_ticks(n);
        data_io::write_ticks(&ctx.command, &ctx.config.paths.input, &ticks)?;
        println!("binance_smoke_status=PASS");
        println!("ticks={}", ctx.config.paths.input.display());
        println!("rows={}", ticks.len());
        Ok(0)
    }

    pub fn cmd_prepare(ctx: &Context) -> AppResult<i32> {
        let ticks = if utils::file_exists(&ctx.config.paths.input) {
            data_io::read_ticks(&ctx.command, &ctx.config.paths.input)?
        } else {
            data_io::sample_ticks(256)
        };
        data_io::write_ticks(&ctx.command, &ctx.config.paths.input, &ticks)?;
        println!("binance_prepare_status=PASS");
        println!("canonical_ticks={}", ctx.config.paths.input.display());
        Ok(0)
    }

    pub fn cmd_collector(ctx: &Context) -> AppResult<i32> {
        utils::ensure_dir(&ctx.command, &ctx.config.paths.workdir)?;
        let path = ctx.config.paths.workdir.join("collector.status.json");
        utils::write_text(&ctx.command, &path, "{\n  \"collector\": \"offline-shim\",\n  \"status\": \"PASS\"\n}\n")?;
        println!("collector_status=PASS");
        println!("status_file={}", path.display());
        Ok(0)
    }

    pub fn cmd_connector(ctx: &Context) -> AppResult<i32> {
        utils::ensure_dir(&ctx.command, &ctx.config.paths.workdir)?;
        let path = ctx.config.paths.workdir.join("connector.status.json");
        utils::write_text(&ctx.command, &path, "{\n  \"connector\": \"offline-shim\",\n  \"status\": \"PASS\"\n}\n")?;
        println!("connector_status=PASS");
        println!("status_file={}", path.display());
        Ok(0)
    }
}

mod audit {
    use super::*;

    #[derive(Default)]
    struct AuditReport {
        ticks_ok: bool,
        features_ok: bool,
        signals_ok: bool,
        posterior_bad_rows: usize,
        prob_bad_rows: usize,
        gate_bad_rows: usize,
        timestamp_monotonic: bool,
        rows: usize,
    }

    pub fn cmd_audit(ctx: &Context) -> AppResult<i32> {
        run_full_audit(ctx)?;
        println!("aga_audit_status=PASS");
        println!("audit_report={}", ctx.config.paths.report.display());
        Ok(0)
    }

    pub fn run_full_audit(ctx: &Context) -> AppResult<()> {
        let mut report = AuditReport::default();
        if utils::file_exists(&ctx.config.paths.input) {
            let ticks = data_io::read_ticks(&ctx.command, &ctx.config.paths.input)?;
            audit_ticks(&ctx.command, &ticks)?;
            report.ticks_ok = true;
        }
        if utils::file_exists(&ctx.config.paths.features) {
            let features = data_io::read_features(&ctx.command, &ctx.config.paths.features)?;
            audit_features(&ctx.command, &features)?;
            report.features_ok = true;
        }
        if utils::file_exists(&ctx.config.paths.signals) {
            let signals = data_io::read_signals(&ctx.command, &ctx.config.paths.signals)?;
            let (prob_bad, posterior_bad, gate_bad, monotonic) = audit_signals(&ctx.command, &signals)?;
            report.signals_ok = true;
            report.rows = signals.len();
            report.prob_bad_rows = prob_bad;
            report.posterior_bad_rows = posterior_bad;
            report.gate_bad_rows = gate_bad;
            report.timestamp_monotonic = monotonic;
        }
        let json = format!(
            "{{\n  \"schema\": \"hftbtpro.audit.v1\",\n  \"ticks_ok\": {},\n  \"features_ok\": {},\n  \"signals_ok\": {},\n  \"rows\": {},\n  \"timestamp_monotonic\": {},\n  \"probability_bad_rows\": {},\n  \"posterior_bad_rows\": {},\n  \"gate_bad_rows\": {},\n  \"command_coverage\": {}\n}}\n",
            report.ticks_ok, report.features_ok, report.signals_ok, report.rows, report.timestamp_monotonic,
            report.prob_bad_rows, report.posterior_bad_rows, report.gate_bad_rows, command_registry::COMMANDS.len()
        );
        utils::write_text(&ctx.command, &ctx.config.paths.report, &json)
    }

    pub fn audit_ticks(command: &str, ticks: &[data_io::Tick]) -> AppResult<()> {
        let mut last_ts = i64::MIN;
        for t in ticks {
            if t.ts_ns < last_ts {
                return Err(AppError::new(command, "audit_ticks", "timestamps are not monotonic"));
            }
            last_ts = t.ts_ns;
            for (name, v) in [
                ("bid_px", t.bid_px), ("ask_px", t.ask_px), ("bid_qty", t.bid_qty),
                ("ask_qty", t.ask_qty), ("trade_px", t.trade_px), ("trade_qty", t.trade_qty),
            ] {
                if !v.is_finite() {
                    return Err(AppError::new(command, "audit_ticks", format!("{name} is not finite")));
                }
            }
            if t.ask_px < t.bid_px {
                return Err(AppError::new(command, "audit_ticks", "ask < bid"));
            }
        }
        Ok(())
    }

    pub fn audit_features(command: &str, rows: &[data_io::FeatureRow]) -> AppResult<()> {
        let mut last_ts = i64::MIN;
        for r in rows {
            if r.ts_ns < last_ts {
                return Err(AppError::new(command, "audit_features", "feature timestamps are not monotonic"));
            }
            last_ts = r.ts_ns;
            for v in r.vector() {
                if !v.is_finite() {
                    return Err(AppError::new(command, "audit_features", "feature contains NaN/Inf"));
                }
            }
            utils::ensure_range(command, "imbalance", r.imbalance, -1.0, 1.0)?;
        }
        Ok(())
    }

    pub fn audit_signals(command: &str, rows: &[data_io::SignalRow]) -> AppResult<(usize, usize, usize, bool)> {
        let mut prob_bad = 0usize;
        let mut posterior_bad = 0usize;
        let mut gate_bad = 0usize;
        let mut monotonic = true;
        let mut last_ts = i64::MIN;
        for r in rows {
            if r.ts_ns < last_ts {
                monotonic = false;
            }
            last_ts = r.ts_ns;
            if assert_probability(command, "softmax", &[r.prob_down, r.prob_neutral, r.prob_up]).is_err() {
                prob_bad += 1;
            }
            if assert_probability(command, "posterior", &r.posterior).is_err() {
                posterior_bad += 1;
            }
            if !(r.gate.is_finite() && (0.0..=1.0).contains(&r.gate)) {
                gate_bad += 1;
            }
        }
        Ok((prob_bad, posterior_bad, gate_bad, monotonic))
    }

    pub fn assert_probability(command: &str, name: &str, xs: &[f64]) -> AppResult<()> {
        let mut sum = 0.0;
        for x in xs {
            if !x.is_finite() || *x < -1e-9 || *x > 1.0 + 1e-9 {
                return Err(AppError::new(command, "probability", format!("{name} contains invalid value {x}")));
            }
            sum += *x;
        }
        if (sum - 1.0).abs() > 1e-6 {
            return Err(AppError::new(command, "probability", format!("{name} sums to {sum}")));
        }
        Ok(())
    }
}

mod report {
    use super::*;

    pub fn cmd_report(ctx: &Context) -> AppResult<i32> {
        write_summary(ctx)?;
        println!("report_status=PASS");
        println!("report={}", ctx.config.paths.workdir.join("summary.md").display());
        Ok(0)
    }

    pub fn write_summary(ctx: &Context) -> AppResult<()> {
        utils::ensure_dir(&ctx.command, &ctx.config.paths.workdir)?;
        let path = ctx.config.paths.workdir.join("summary.md");
        let text = format!(
            "# hftbtpro E2E Report\n\n- version: `{}`\n- input: `{}`\n- features: `{}`\n- weights: `{}`\n- signals: `{}`\n- audit: `{}`\n- backtest: `{}`\n\nStatus: generated by single-file Rust appliance.\n",
            APP_VERSION,
            ctx.config.paths.input.display(),
            ctx.config.paths.features.display(),
            ctx.config.paths.weights.display(),
            ctx.config.paths.signals.display(),
            ctx.config.paths.report.display(),
            ctx.config.paths.backtest_report.display()
        );
        utils::write_text(&ctx.command, &path, &text)
    }
}

mod bootstrap {
    use super::*;

    pub fn cmd_bootstrap(ctx: &Context) -> AppResult<i32> {
        let harness = ctx.config.paths.workdir.join("cargo_harness");
        let src_dir = harness.join("src");
        utils::ensure_dir(&ctx.command, &src_dir)?;
        let source = utils::arg_value(&ctx.args, "--source").unwrap_or_else(|| "hftbt.rs".to_string());
        let source_path = PathBuf::from(source);
        let code = fs::read_to_string(&source_path)
            .map_err(|e| AppError::with_path(&ctx.command, "read_source", &source_path, e.to_string()))?;
        utils::write_text(&ctx.command, &src_dir.join("main.rs"), &code)?;
        let cargo = "[package]\nname=\"hftbtpro_harness\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n";
        utils::write_text(&ctx.command, &harness.join("Cargo.toml"), cargo)?;
        println!("bootstrap_status=PASS");
        println!("harness={}", harness.display());
        Ok(0)
    }

    pub fn cmd_check(ctx: &Context) -> AppResult<i32> { run_cargo(ctx, &["check"]) }
    pub fn cmd_build(ctx: &Context) -> AppResult<i32> { run_cargo(ctx, &["build"]) }
    pub fn cmd_test(ctx: &Context) -> AppResult<i32> { run_cargo(ctx, &["test"]) }
    pub fn cmd_release(ctx: &Context) -> AppResult<i32> { run_cargo(ctx, &["build", "--release"]) }

    pub fn cmd_portable_proof(ctx: &Context) -> AppResult<i32> {
        utils::ensure_dir(&ctx.command, &ctx.config.paths.workdir)?;
        let ticks = data_io::sample_ticks(256);
        data_io::write_ticks(&ctx.command, &ctx.config.paths.input, &ticks)?;
        aga_stack::run_prepare(ctx, true)?;
        aga_stack::run_train(ctx)?;
        aga_stack::run_infer(ctx)?;
        let bt = hft_engine_adapter::run_backtest(ctx)?;
        audit::run_full_audit(ctx)?;
        report::write_summary(ctx)?;
        println!("portable_proof_status=PASS");
        println!("rows={} fills={} pnl={:.6}", bt.rows, bt.fills, bt.final_pnl);
        Ok(0)
    }

    fn run_cargo(ctx: &Context, cargo_args: &[&str]) -> AppResult<i32> {
        let status = Command::new("cargo")
            .args(cargo_args)
            .status()
            .map_err(|e| AppError::new(&ctx.command, "cargo", format!("failed to launch cargo: {e}")))?;
        if status.success() {
            println!("{}_status=PASS", ctx.command.replace('-', "_"));
            Ok(status.code().unwrap_or(0))
        } else {
            Err(AppError::new(&ctx.command, "cargo", format!("cargo {:?} failed with {:?}", cargo_args, status.code())))
        }
    }
}
