use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::api::{ExchangePrice, Side, TradingPair};
use crate::util::{parse_price_cents, parse_quantity_smallest_unit};

const BITSTAMP_WS_URL: &str = "wss://ws.bitstamp.net";

pub struct BitstampClient {
    tx: mpsc::Sender<ExchangePrice>,
}

impl BitstampClient {
    pub fn new(tx: mpsc::Sender<ExchangePrice>) -> Self {
        BitstampClient { tx }
    }

    /// Listen to a specific trading pair's order book on Bitstamp.
    pub async fn listen_pair(&self, pair: TradingPair) {
        match connect_async(BITSTAMP_WS_URL).await {
            Ok((mut ws_stream, _)) => {
                println!(
                    "[Bitstamp] Connected to {} for pair {}",
                    BITSTAMP_WS_URL,
                    pair.as_str()
                );

                let channel = format!("order_book_{}", pair.bitstamp_pair_code());

                let subscribe_msg = serde_json::json!({
                    "event": "bts:subscribe",
                    "data": {
                        "channel": channel
                    }
                });

                if let Err(e) = ws_stream
                    .send(Message::Text(subscribe_msg.to_string()))
                    .await
                {
                    println!("[Bitstamp] failed to send subscription: {e}");
                    return;
                }

                let (_write, mut read) = ws_stream.split();

                let mut received_any = false;

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            println!("[Bitstamp] raw text: {}", text);
                            let received_at = Self::current_timestamp_ms();
                            if let Err(e) = self.handle_message(&text, received_at).await {
                                println!("[Bitstamp] error handling message: {e}");
                            } else {
                                received_any = true;
                            }
                        }
                        Ok(Message::Ping(_data)) => {
                            println!("[Bitstamp] received ping");
                        }
                        Ok(Message::Close(_)) => {
                            println!("[Bitstamp] websocket closed by server");
                            break;
                        }
                        Err(e) => {
                            println!("[Bitstamp] websocket error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }

                if !received_any {
                    println!(
                        "[Bitstamp] No order book messages received for pair {}. Check symbol or channel.",
                        pair.as_str()
                    );
                }
            }
            Err(e) => {
                println!("[Bitstamp] failed to connect to {}: {e}", BITSTAMP_WS_URL);
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

        let event = match v.get("event").and_then(|e| e.as_str()) {
            Some(e) => e,
            None => return Ok(()),
        };

        // Ignore non-data events (subscription acks, reconnects, etc.)
        if event != "data" {
            return Ok(());
        }

        let data = match v.get("data") {
            Some(d) => d,
            None => return Ok(()),
        };

        let exchange_timestamp = data
            .get("microtimestamp")
            .and_then(|t| t.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Bids: [["price", "amount"], ...]
        if let Some(bids) = data.get("bids").and_then(|b| b.as_array()) {
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
                                    .send(ExchangePrice::Bitstamp {
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

        // Asks: [["price", "amount"], ...]
        if let Some(asks) = data.get("asks").and_then(|a| a.as_array()) {
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
                                    .send(ExchangePrice::Bitstamp {
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

        Ok(())
    }
}
