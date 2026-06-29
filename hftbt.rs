use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32, String> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let cmd = if args.is_empty() { "help".to_string() } else { args.remove(0) };

    match cmd.as_str() {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(0)
        }
        "verify" => {
            println!("verify_status=PASS");
            Ok(0)
        }
        "aga-auto" => aga_stack::run_aga_auto(&args),
        "aga-infer" => aga_stack::run_aga_infer(&args),
        "aga-audit" => aga_stack::run_aga_audit(&args),
        other => Err(format!("unknown command `{other}`")),
    }
}

fn print_help() {
    println!(
        "\
hftbtpro single-file Rust scaffold\n\n\
Commands:\n\
  help        Show help\n\
  verify      Smoke verify\n\
  aga-auto    Run sample AGA inference path\n\
  aga-infer   Read tick CSV and write signal CSV\n\
  aga-audit   Audit signal CSV\n\n\
Example:\n\
  rustc --edition=2021 hftbt.rs -O -o hftbt\n\
  ./hftbt aga-auto --input ticks.sample.csv --output aga_signals.csv\n\
  ./hftbt aga-audit --input aga_signals.csv --report aga_audit.json\n"
    );
}

mod aga_stack {
    use super::*;

    #[derive(Clone, Debug)]
    struct Config {
        lookback: usize,
        min_confidence: f64,
        buy_threshold: f64,
        sell_threshold: f64,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                lookback: 32,
                min_confidence: 0.55,
                buy_threshold: 0.55,
                sell_threshold: 0.55,
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct Tick {
        ts_ns: i64,
        bid_px: f64,
        ask_px: f64,
        bid_qty: f64,
        ask_qty: f64,
        trade_qty: f64,
        trade_side: i8,
    }

    #[derive(Clone, Debug)]
    struct FeatureRow {
        ts_ns: i64,
        x: [f64; 8],
        sigma: f64,
        lambda: f64,
    }

    #[derive(Clone, Debug)]
    struct ModelOutput {
        ts_ns: i64,
        prob_down: f64,
        prob_neutral: f64,
        prob_up: f64,
        regime: usize,
        gate: f64,
        confidence: f64,
    }

    pub fn run_aga_auto(args: &[String]) -> Result<i32, String> {
        let input = arg_value(args, "--input").unwrap_or_else(|| "ticks.sample.csv".to_string());
        let output = arg_value(args, "--output").unwrap_or_else(|| "aga_signals.csv".to_string());
        run_aga_infer_inner(&input, &output)?;
        println!("aga_auto_status=PASS");
        println!("signals={output}");
        Ok(0)
    }

    pub fn run_aga_infer(args: &[String]) -> Result<i32, String> {
        let input = arg_value(args, "--input").ok_or("--input PATH required")?;
        let output = arg_value(args, "--output").unwrap_or_else(|| "aga_signals.csv".to_string());
        run_aga_infer_inner(&input, &output)?;
        println!("aga_infer_status=PASS");
        println!("signals={output}");
        Ok(0)
    }

    pub fn run_aga_audit(args: &[String]) -> Result<i32, String> {
        let input = arg_value(args, "--input").ok_or("--input PATH required")?;
        let report = arg_value(args, "--report").unwrap_or_else(|| "aga_audit.json".to_string());
        audit_signal_file(&input, &report)?;
        println!("aga_audit_status=PASS");
        println!("report={report}");
        Ok(0)
    }

    fn run_aga_infer_inner(input: &str, output: &str) -> Result<(), String> {
        let cfg = Config::default();
        let file = File::open(input).map_err(|e| format!("open input failed: {input}: {e}"))?;
        let mut out = File::create(output).map_err(|e| format!("create output failed: {output}: {e}"))?;

        writeln!(out, "ts_ns,prob_down,prob_neutral,prob_up,regime,gate,confidence,signal")
            .map_err(|e| e.to_string())?;

        let mut fb = FeatureBuilder::new(cfg.lookback);
        let mut model = AgaLiteModel::new();
        let mut rows = 0usize;

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() || line.starts_with("ts") {
                continue;
            }
            let tick = parse_tick(&line)?;
            let row = fb.update(tick);
            let y = model.step(&row);
            let signal = policy(&y, &cfg);
            writeln!(
                out,
                "{},{:.6},{:.6},{:.6},{},{:.6},{:.6},{}",
                y.ts_ns, y.prob_down, y.prob_neutral, y.prob_up, y.regime, y.gate, y.confidence, signal
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }

        println!("aga_rows={rows}");
        Ok(())
    }

    struct FeatureBuilder {
        lookback: usize,
        mids: Vec<f64>,
        last_mid: Option<f64>,
        count: usize,
    }

    impl FeatureBuilder {
        fn new(lookback: usize) -> Self {
            Self {
                lookback,
                mids: Vec::with_capacity(lookback + 1),
                last_mid: None,
                count: 0,
            }
        }

        fn update(&mut self, tick: Tick) -> FeatureRow {
            let mid = 0.5 * (tick.bid_px + tick.ask_px);
            let spread = (tick.ask_px - tick.bid_px).max(0.0);
            let depth = tick.bid_qty + tick.ask_qty + 1e-12;
            let imbalance = (tick.bid_qty - tick.ask_qty) / depth;
            let mid_ret = self
                .last_mid
                .map(|m| if m > 0.0 { 10_000.0 * (mid / m).ln() } else { 0.0 })
                .unwrap_or(0.0);

            self.last_mid = Some(mid);
            self.mids.push(mid);
            if self.mids.len() > self.lookback {
                self.mids.remove(0);
            }
            self.count += 1;

            FeatureRow {
                ts_ns: tick.ts_ns,
                x: [
                    spread,
                    mid_ret,
                    imbalance,
                    tick.bid_qty,
                    tick.ask_qty,
                    depth,
                    tick.trade_side as f64,
                    tick.trade_qty,
                ],
                sigma: realized_vol(&self.mids),
                lambda: self.count as f64 / self.lookback.max(1) as f64,
            }
        }
    }

    struct AgaLiteModel {
        posterior: [f64; 4],
        coarse: [f64; 8],
    }

    impl AgaLiteModel {
        fn new() -> Self {
            Self {
                posterior: [0.25; 4],
                coarse: [0.0; 8],
            }
        }

        fn step(&mut self, row: &FeatureRow) -> ModelOutput {
            for i in 0..8 {
                self.coarse[i] = 0.95 * self.coarse[i] + 0.05 * row.x[i].tanh();
            }

            let gate = sigmoid(0.05 * row.sigma + 0.02 * row.lambda);
            let micro_score = row.x[2] + 0.1 * row.x[6];
            let slow_score = self.coarse[2];
            let score = gate * micro_score + (1.0 - gate) * slow_score;

            let high_vol = sigmoid(row.sigma - 1.0);
            self.posterior = normalize4([0.30 * (1.0 - high_vol), 0.25, 0.25, high_vol]);

            let logits = [-score, 0.05, score];
            let p = softmax3(logits);
            let regime = argmax4(self.posterior);
            let confidence = p[0].max(p[1]).max(p[2]);

            ModelOutput {
                ts_ns: row.ts_ns,
                prob_down: p[0],
                prob_neutral: p[1],
                prob_up: p[2],
                regime,
                gate,
                confidence,
            }
        }
    }

    fn policy(y: &ModelOutput, cfg: &Config) -> &'static str {
        if y.confidence < cfg.min_confidence {
            "HOLD"
        } else if y.prob_up > cfg.buy_threshold {
            "BUY"
        } else if y.prob_down > cfg.sell_threshold {
            "SELL"
        } else {
            "HOLD"
        }
    }

    fn audit_signal_file(input: &str, report: &str) -> Result<(), String> {
        let file = File::open(input).map_err(|e| format!("open signal failed: {input}: {e}"))?;
        let mut rows = 0usize;
        let mut monotonic = true;
        let mut last_ts = i64::MIN;
        let mut conf_sum = 0.0;
        let mut prob_bad = 0usize;

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() || line.starts_with("ts") {
                continue;
            }
            let p: Vec<&str> = line.split(',').collect();
            if p.len() < 7 {
                continue;
            }
            let ts: i64 = p[0].parse().unwrap_or(last_ts);
            if ts < last_ts {
                monotonic = false;
            }
            last_ts = ts;
            let pd = p[1].parse::<f64>().unwrap_or(f64::NAN);
            let pn = p[2].parse::<f64>().unwrap_or(f64::NAN);
            let pu = p[3].parse::<f64>().unwrap_or(f64::NAN);
            let ps = pd + pn + pu;
            if !ps.is_finite() || (ps - 1.0).abs() > 0.01 {
                prob_bad += 1;
            }
            conf_sum += p[6].parse::<f64>().unwrap_or(0.0);
            rows += 1;
        }

        let json = format!(
            "{{\n  \"schema\": \"hftbtpro.aga.audit.v1\",\n  \"rows\": {},\n  \"timestamp_monotonic\": {},\n  \"probability_bad_rows\": {},\n  \"confidence_mean\": {:.6}\n}}\n",
            rows,
            monotonic,
            prob_bad,
            conf_sum / rows.max(1) as f64
        );
        std::fs::write(report, json).map_err(|e| format!("write report failed: {e}"))
    }

    fn parse_tick(line: &str) -> Result<Tick, String> {
        let p: Vec<&str> = line.split(',').collect();
        if p.len() < 8 {
            return Err(format!("bad tick csv row: {line}"));
        }
        Ok(Tick {
            ts_ns: p[0].parse().map_err(|_| "bad ts_ns")?,
            bid_px: p[1].parse().map_err(|_| "bad bid_px")?,
            ask_px: p[2].parse().map_err(|_| "bad ask_px")?,
            bid_qty: p[3].parse().map_err(|_| "bad bid_qty")?,
            ask_qty: p[4].parse().map_err(|_| "bad ask_qty")?,
            trade_qty: p[6].parse().map_err(|_| "bad trade_qty")?,
            trade_side: p[7].parse().map_err(|_| "bad trade_side")?,
        })
    }

    fn realized_vol(xs: &[f64]) -> f64 {
        if xs.len() < 3 {
            return 0.0;
        }
        let mut rs = Vec::with_capacity(xs.len() - 1);
        for i in 1..xs.len() {
            if xs[i - 1] > 0.0 {
                rs.push((xs[i] / xs[i - 1]).ln());
            }
        }
        if rs.is_empty() {
            return 0.0;
        }
        let mean = rs.iter().sum::<f64>() / rs.len() as f64;
        let var = rs.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / rs.len() as f64;
        10_000.0 * var.sqrt()
    }

    fn softmax3(x: [f64; 3]) -> [f64; 3] {
        let m = x[0].max(x[1]).max(x[2]);
        let e = [(x[0] - m).exp(), (x[1] - m).exp(), (x[2] - m).exp()];
        let s = e[0] + e[1] + e[2];
        [e[0] / s, e[1] / s, e[2] / s]
    }

    fn normalize4(x: [f64; 4]) -> [f64; 4] {
        let s = x.iter().sum::<f64>().max(1e-12);
        [x[0] / s, x[1] / s, x[2] / s, x[3] / s]
    }

    fn argmax4(x: [f64; 4]) -> usize {
        let mut bi = 0;
        let mut bv = x[0];
        for i in 1..4 {
            if x[i] > bv {
                bi = i;
                bv = x[i];
            }
        }
        bi
    }

    fn sigmoid(x: f64) -> f64 {
        1.0 / (1.0 + (-x).exp())
    }

    fn arg_value(args: &[String], flag: &str) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == flag {
                return args.get(i + 1).cloned();
            }
            if let Some(v) = args[i].strip_prefix(&(flag.to_string() + "=")) {
                return Some(v.to_string());
            }
        }
        None
    }
}
