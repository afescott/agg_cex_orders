mod api;
mod util;

use std::env;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // Read trading pair from env, defaulting to a common pair when missing.
    let pair = match env::var("TRADING_PAIR") {
        Ok(s) => match api::TradingPair::from_str(&s) {
            Some(p) => p,
            None => {
                eprintln!(
                    "TRADING_PAIR env var is empty. Expected something like 'ETH/USDT' or 'BTC-USDT'. Exiting."
                );
                return;
            }
        },
        Err(_) => return,
    };

    // Create a channel to receive price updates from exchanges
    let (tx, mut rx) = mpsc::channel::<api::ExchangePrice>(1000);

    // Spawn Binance listener
    let binance_tx = tx.clone();
    let binance_pair = pair.clone();
    let binance_handle = tokio::spawn(async move {
        let client = api::binance::BinanceClient::new(binance_tx);
        client.listen_pair(binance_pair).await;
    });

    // Spawn Coinbase listener
    let coinbase_tx = tx.clone();
    let coinbase_pair = pair;
    let coinbase_handle = tokio::spawn(async move {
        let client = api::coinbase::CoinbaseClient::new(coinbase_tx);
        client.listen_pair(coinbase_pair).await;
    });

    // Spawn consumer of aggregated prices
    let receiver_handle = tokio::spawn(async move {
        while let Some(price) = rx.recv().await {
            println!("Received price update: {:?}", price);
            // Handle price update - could send to gRPC API, log, etc.
        }
    });

    let _ = tokio::join!(binance_handle, coinbase_handle, receiver_handle);
}
