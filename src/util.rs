//! Utilities: parsing, and tracing/tokio-console/flamegraph setup.

use std::env;
use std::io::BufWriter;
use std::time::Duration as StdDuration;
use tracing_subscriber::prelude::*;

/// Sets up tracing: tokio-console (with tokio/runtime at TRACE), fmt layer (stdout),
/// and optionally a flame layer when `TRACING_FLAME=1`. Returns a guard that must be
/// held until process exit so the flamegraph file is flushed; callers should
/// `let _guard = util::setup_config();`.
pub fn setup_config() -> Option<tracing_flame::FlushGuard<BufWriter<std::fs::File>>> {
    // Tokio/runtime at TRACE for the console; keep them off stdout.
    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("tokio=trace".parse().expect("tokio directive"))
        .add_directive("runtime=trace".parse().expect("runtime directive"));
    let filter = if env::var("TRACING_FLAME").is_ok() {
        filter.add_directive(
            "websocket_agg_orders=trace"
                .parse()
                .expect("crate directive"),
        )
    } else {
        filter
    };

    let fmt_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("tokio=warn".parse().expect("tokio warn"))
        .add_directive("runtime=warn".parse().expect("runtime warn"));

    let console_layer = console_subscriber::ConsoleLayer::builder()
        .with_default_env()
        .retention(StdDuration::from_secs(3600))
        .spawn();

    let (flame_layer, flame_guard) = match env::var("TRACING_FLAME") {
        Ok(_) => match tracing_flame::FlameLayer::with_file("./flamegraph.folded") {
            Ok((layer, guard)) => (Some(layer), Some(guard)),
            Err(e) => {
                eprintln!("TRACING_FLAME: could not create flamegraph file: {e}");
                (None, None)
            }
        },
        Err(_) => (None, None),
    };

    if let Some(flame) = flame_layer {
        tracing_subscriber::registry()
            .with(filter)
            .with(console_layer)
            .with(flame)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_filter(fmt_filter),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(console_layer)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_filter(fmt_filter),
            )
            .init();
    }

    flame_guard
}

/// Parse a decimal price string into cents (2 decimal places).
/// Returns `None` if the string cannot be parsed.
pub fn parse_price_cents(s: &str) -> Option<u64> {
    let mut parts = s.split('.');
    let int_part = parts.next()?;
    let frac_part = parts.next();

    // More than one '.' is considered invalid
    if parts.next().is_some() {
        return None;
    }

    let int_val: u64 = int_part.parse().ok()?;

    let frac_val = if let Some(frac) = frac_part {
        let mut frac = frac.to_string();
        // We only care about 2 decimal places for "cents"
        if frac.len() > 2 {
            frac.truncate(2);
        } else {
            while frac.len() < 2 {
                frac.push('0');
            }
        }
        frac.parse::<u64>().ok()?
    } else {
        0
    };

    int_val.checked_mul(100)?.checked_add(frac_val)
}

/// Parse a decimal quantity string into the smallest unit given by `decimals`.
/// For example, with `decimals = 8`, "0.00000001" becomes 1.
pub fn parse_quantity_smallest_unit(s: &str, decimals: u32) -> Option<u64> {
    let mut parts = s.split('.');
    let int_part = parts.next()?;
    let frac_part = parts.next();

    // More than one '.' is considered invalid
    if parts.next().is_some() {
        return None;
    }

    let int_val: u64 = int_part.parse().ok()?;

    let scale = 10u64.checked_pow(decimals)?;

    let frac_val = if let Some(frac) = frac_part {
        let mut frac = frac.to_string();
        if frac.len() > decimals as usize {
            frac.truncate(decimals as usize);
        } else {
            while frac.len() < decimals as usize {
                frac.push('0');
            }
        }
        frac.parse::<u64>().ok()?
    } else {
        0
    };

    int_val.checked_mul(scale)?.checked_add(frac_val)
}
