mod api;
mod orderbook;
mod util;

use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use orderbook::OrderBook;

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

    let orderbook = Arc::new(OrderBook::new(pair.as_str().to_string()));

    // Create a channel to receive price updates from exchanges
    let (tx, mut rx) = mpsc::channel::<api::ExchangePrice>(1000);

    // Spawn Binance listener
    let binance_tx = tx.clone();
    let binance_pair = pair.clone();
    let binance_handle = tokio::spawn(async move {
        let client = api::binance::BinanceClient::new(binance_tx);
        client.listen_pair(binance_pair).await;
    });

    // Spawn Bitstamp listener
    let bitstamp_tx = tx.clone();
    let bitstamp_pair = pair;
    let bitstamp_handle = tokio::spawn(async move {
        let client = api::bitstamp::BitstampClient::new(bitstamp_tx);
        client.listen_pair(bitstamp_pair).await;
    });

    // Spawn consumer of aggregated prices that updates the in-memory order book
    let ob = orderbook.clone();
    let receiver_handle = tokio::spawn(async move {
        while let Some(price) = rx.recv().await {
            ob.update_price_level(price);
        }
    });

    let _ = tokio::join!(binance_handle, bitstamp_handle, receiver_handle);
}
