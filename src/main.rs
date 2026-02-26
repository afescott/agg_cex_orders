mod api;
mod orderbook;
mod util;

use orderbook::OrderBook;
use std::env;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    // Read trading pair from env, defaulting to a common pair when missing/invalid.
    let pair = match env::var("TRADING_PAIR") {
        Ok(s) => match api::TradingPair::from_str(&s) {
            Some(p) => p,
            None => {
                eprintln!(
                    "TRADING_PAIR is invalid or empty (got '{}'); defaulting to BTC-USDT.",
                    s
                );
                api::TradingPair::default_pair()
            }
        },
        Err(_) => {
            eprintln!("TRADING_PAIR not set; defaulting to BTC-USDT.");
            api::TradingPair::default_pair()
        }
    };

    let orderbook = Arc::new(OrderBook::new(pair.as_str().to_string()));

    // How long to run the feeds before taking a snapshot.
    let run_duration = Duration::from_secs(10);

    // Create a channel to receive price updates from exchanges
    let (tx, mut rx) = mpsc::channel::<api::ExchangePrice>(1000);

    // Spawn Binance listener (with small sync delay so both exchanges start together)
    let binance_tx = tx.clone();
    let binance_pair = pair.clone();
    let binance_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        let client = api::binance::BinanceClient::new(binance_tx);
        client.listen_pair(binance_pair).await;
    });

    // Spawn Bitstamp listener (same delay as Binance)
    let bitstamp_tx = tx.clone();
    let bitstamp_pair = pair;
    let bitstamp_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        let client = api::bitstamp::BitstampClient::new(bitstamp_tx);
        client.listen_pair(bitstamp_pair).await;
    });

    // We no longer need our own sender handle in main.
    drop(tx);

    // Drive the order book updates until either:
    // - the time window elapses, or
    // - all senders are dropped and the channel closes.
    let window = sleep(run_duration);
    tokio::pin!(window);

    // Also listen for Ctrl+C so we can take a snapshot on manual shutdown.
    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            maybe_price = rx.recv() => {
                match maybe_price {
                    Some(price) => {
                        orderbook.update_price_level(price);
                    }
                    None => {
                        // All senders closed; nothing more to aggregate.
                        break;
                    }
                }
            }
            _ = &mut window => {
                // Time window elapsed.
                break;
            }
            _ = &mut ctrl_c => {
                eprintln!("Ctrl+C received, shutting down and printing current book snapshot...");
                break;
            }
        }
    }

    // Graceful-ish shutdown: stop exchange tasks.
    binance_handle.abort();
    bitstamp_handle.abort();

    // Take a final snapshot of the combined book.
    let top_bids = orderbook.top_bids_all_exchanges();
    let top_asks = orderbook.top_asks_all_exchanges();
    let spread_cents = orderbook.spread_all_exchanges();

    // Convert to JSON-style output like the example.
    let bids_json: Vec<_> = top_bids
        .into_iter()
        .map(|(exchange, price_cents, qty_smallest)| {
            let exchange_str = match exchange {
                api::Exchange::Binance => "binance",
                api::Exchange::Bitstamp => "bitstamp",
            };
            serde_json::json!({
                "exchange": exchange_str,
                "price": price_cents as f64 / 100.0,
                "amount": qty_smallest as f64 / 1e8, // assuming 8 decimals
            })
        })
        .collect();

    let asks_json: Vec<_> = top_asks
        .into_iter()
        .map(|(exchange, price_cents, qty_smallest)| {
            let exchange_str = match exchange {
                api::Exchange::Binance => "binance",
                api::Exchange::Bitstamp => "bitstamp",
            };
            serde_json::json!({
                "exchange": exchange_str,
                "price": price_cents as f64 / 100.0,
                "amount": qty_smallest as f64 / 1e8, // assuming 8 decimals
            })
        })
        .collect();

    let snapshot = serde_json::json!({
        "spread": spread_cents.map(|c| c as f64 / 100.0),
        "asks": asks_json,
        "bids": bids_json,
    });

    println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
}
