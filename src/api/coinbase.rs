use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::api::{ExchangePrice, Side, TradingPair};
use crate::util::{parse_price_cents, parse_quantity_smallest_unit};

const COINBASE_WS_URL: &str = "wss://ws-feed.exchange.coinbase.com";

pub struct CoinbaseClient {
    tx: mpsc::Sender<ExchangePrice>,
}

impl CoinbaseClient {
    pub fn new(tx: mpsc::Sender<ExchangePrice>) -> Self {
        CoinbaseClient { tx }
    }

    /// Listen to a specific trading pair's level2 order book on Coinbase.
    pub async fn listen_pair(&self, pair: TradingPair) {
        match connect_async(COINBASE_WS_URL).await {
            Ok((mut ws_stream, _)) => {
                println!(
                    "[Coinbase] Connected to {} for pair {}",
                    COINBASE_WS_URL,
                    pair.as_str()
                );

                // Subscribe to `<product_id>` level2 order book
                let product_id = pair.coinbase_product_id();
                let subscribe_msg = serde_json::json!({
                    "type": "subscribe",
                    "product_ids": [product_id],
                    "channels": ["level2"]
                });

                if let Err(e) = ws_stream
                    .send(Message::Text(subscribe_msg.to_string()))
                    .await
                {
                    println!("[Coinbase] failed to send subscription: {e}");
                    return;
                }

                let (_write, mut read) = ws_stream.split();

                let mut received_any = false;

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            received_any = true;
                            println!("[Coinbase] raw text: {}", text);
                            let received_at = Self::current_timestamp_ms();
                            if let Err(e) = self.handle_message(&text, received_at).await {
                                println!("[Coinbase] error handling message: {e}");
                            }
                        }
                        Ok(Message::Ping(_data)) => {
                            println!("[Coinbase] received ping");
                        }
                        Ok(Message::Close(_)) => {
                            println!("[Coinbase] websocket closed by server");
                            break;
                        }
                        Err(e) => {
                            println!("[Coinbase] websocket error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }

                if !received_any {
                    println!(
                        "[Coinbase] No messages received for pair {}. The product may require auth or may not exist.",
                        pair.as_str()
                    );
                }
            }
            Err(e) => {
                println!("[Coinbase] failed to connect to {}: {e}", COINBASE_WS_URL);
            }
        }
    }

    /// Get the current time as milliseconds since Unix epoch.
    fn current_timestamp_ms() -> u64 {
        let now = std::time::SystemTime::now();
        now.duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    async fn handle_message(
        &self,
        text: &str,
        received_at: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if text.len() > 100_000 {
            return Err("Message too large".into());
        }

        let v: serde_json::Value = serde_json::from_str(text)?;

        let msg_type = match v.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return Ok(()),
        };

        // Ignore subscription acks etc.
        if msg_type == "subscriptions" {
            return Ok(());
        }

        // For now, we don't parse Coinbase's exchange timestamp; keep 0 for parity with Binance.
        let exchange_timestamp: u64 = 0;

        // Initial snapshot: bids/asks arrays
        if msg_type == "snapshot" {
            if let Some(bids) = v.get("bids").and_then(|b| b.as_array()) {
                for bid in bids {
                    if let Some(arr) = bid.as_array() {
                        if arr.len() >= 2 {
                            if let (Some(price_str), Some(size_str)) =
                                (arr[0].as_str(), arr[1].as_str())
                            {
                                if size_str == "0" {
                                    continue;
                                }
                                let price_opt = parse_price_cents(price_str);
                                let quantity_opt = parse_quantity_smallest_unit(size_str, 8);

                                if let (Some(price), Some(quantity)) = (price_opt, quantity_opt) {
                                    let _ = self
                                        .tx
                                        .send(ExchangePrice::Coinbase {
                                            price,
                                            quantity,
                                            exchange_timestamp,
                                            received_at,
                                            side: Side::Buy,
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(asks) = v.get("asks").and_then(|a| a.as_array()) {
                for ask in asks {
                    if let Some(arr) = ask.as_array() {
                        if arr.len() >= 2 {
                            if let (Some(price_str), Some(size_str)) =
                                (arr[0].as_str(), arr[1].as_str())
                            {
                                if size_str == "0" {
                                    continue;
                                }
                                let price_opt = parse_price_cents(price_str);
                                let quantity_opt = parse_quantity_smallest_unit(size_str, 8);

                                if let (Some(price), Some(quantity)) = (price_opt, quantity_opt) {
                                    let _ = self
                                        .tx
                                        .send(ExchangePrice::Coinbase {
                                            price,
                                            quantity,
                                            exchange_timestamp,
                                            received_at,
                                            side: Side::Sell,
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }

            return Ok(());
        }

        // Incremental updates: "l2update" with "changes": [[side, price, size], ...]
        if msg_type == "l2update" {
            if let Some(changes) = v.get("changes").and_then(|c| c.as_array()) {
                for change in changes {
                    if let Some(arr) = change.as_array() {
                        if arr.len() >= 3 {
                            if let (Some(side_str), Some(price_str), Some(size_str)) =
                                (arr[0].as_str(), arr[1].as_str(), arr[2].as_str())
                            {
                                if size_str == "0" {
                                    continue;
                                }

                                let side = match side_str {
                                    "buy" => Side::Buy,
                                    "sell" => Side::Sell,
                                    _ => continue,
                                };

                                let price_opt = parse_price_cents(price_str);
                                let quantity_opt = parse_quantity_smallest_unit(size_str, 8);

                                if let (Some(price), Some(quantity)) = (price_opt, quantity_opt) {
                                    let _ = self
                                        .tx
                                        .send(ExchangePrice::Coinbase {
                                            price,
                                            quantity,
                                            exchange_timestamp,
                                            received_at,
                                            side,
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

