mod api;

use std::env;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // Create a channel to receive price updates from exchanges
    let (tx, mut rx) = mpsc::channel::<api::ExchangePrice>(1000);

    let binance_tx = tx.clone();
    let handle = tokio::spawn(async move {
        let client = api::binance::BinanceClient::new(binance_tx);
        client.listen_btc_usdt().await;
    });

    let coinbase_tx = tx.clone();
    let handle = tokio::spawn(async move {
        let client = api::coinbase::CoinbaseClient::new(coinbase_tx);
        client.listen_btc_usd().await;
    });

    tokio::spawn(async move {
        while let Some(price) = rx.recv().await {
            // Handle price update - could send to gRPC API
            // deploy_api.receive_price(price).await;
        }
    });

    // Wait for all exchange connections
    for handle in handles {
        let _ = handle.await;
    }
}
