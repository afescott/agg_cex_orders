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
    let _flame_guard = util::setup_config();

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

    // Start gRPC server that streams summaries from the same in-memory order book.
    let grpc_ob = orderbook.clone();
    let mut grpc_handle = tokio::spawn(async move {
        if let Err(e) = api::grpc::run_grpc_server(grpc_ob).await {
            eprintln!("gRPC server error: {e}");
        }
    });

    // Create a channel to receive price updates from exchanges
    let (tx, mut rx) = mpsc::channel::<api::ExchangePrice>(1000);

    // Spawn Binance listener (with small sync delay so both exchanges start together)
    let binance_tx = tx.clone();
    let binance_pair = pair.clone();
    let mut binance_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        let client = api::binance::BinanceClient::new(binance_tx);
        client.listen_pair(binance_pair).await;
    });

    // Spawn Bitstamp listener (same delay as Binance)
    let bitstamp_tx = tx.clone();
    let bitstamp_pair = pair;
    let mut bitstamp_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        let client = api::bitstamp::BitstampClient::new(bitstamp_tx);
        client.listen_pair(bitstamp_pair).await;
    });

    // We no longer need our own sender handle in main.
    drop(tx);

    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            maybe_price = rx.recv() => {
                match maybe_price {
                    Some(price) => {
                        let _span = tracing::info_span!("update_book", exchange = %price.exchange_name()).entered();
                        orderbook.update_price_level(price);
                    }
                    None => {
                        // All senders closed; nothing more to aggregate.
                        break;
                    }
                }
            }
            _ = &mut ctrl_c => break,
            _ = &mut binance_handle => break,
            _ = &mut bitstamp_handle => break,
            _ = &mut grpc_handle => break,
        }
    }

    // Graceful-ish shutdown: stop exchange tasks.
    binance_handle.abort();
    bitstamp_handle.abort();

    // Take and print a final snapshot of the combined book.
    orderbook.print_snapshot_json();
}
