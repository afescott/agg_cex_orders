use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::api::{ExchangePrice, Side, TradingPair};
use crate::util::{parse_price_cents, parse_quantity_smallest_unit};

const BINANCE_WS_BASE_URL: &str = "wss://stream.binance.com:9443/ws";

pub struct BinanceClient {
    tx: mpsc::Sender<ExchangePrice>,
}

impl BinanceClient {
    pub fn new(tx: mpsc::Sender<ExchangePrice>) -> Self {
        BinanceClient { tx }
    }

    /// Listen to a specific trading pair's depth stream on Binance.
    pub async fn listen_pair(&self, pair: TradingPair) {
        let symbol = pair.binance_symbol();
        let stream_name = format!("{}@depth20@100ms", symbol);
        let url = format!("{}/{}", BINANCE_WS_BASE_URL, stream_name);

        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                println!("[Binance] Connected to {} for pair {}", url, pair.as_str());
                let (_write, mut read) = ws_stream.split();

                let mut received_any = false;

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            received_any = true;
                            println!("[Binance] raw text: {}", text);
                            // Capture timestamp immediately when message received
                            let received_at = Self::current_timestamp_ms();
                            if let Err(_e) = self.handle_message(&text, received_at).await {
                                // Handle or log parsing / channel errors if needed
                            }
                        }
                        Ok(Message::Ping(_data)) => {
                            println!("[Binance] received ping");
                            // Pings can be ignored here; tungstenite handles pongs on the write side.
                        }
                        Ok(Message::Close(_)) => {
                            println!("[Binance] websocket closed by server");
                            break;
                        }
                        Err(e) => {
                            println!("[Binance] websocket error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }

                if !received_any {
                    println!(
                        "[Binance] No messages received for pair {}. Check symbol.",
                        pair.as_str()
                    );
                }
            }
            Err(e) => {
                println!("[Binance] failed to connect to {}: {e}", url);
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
        // Basic validation - avoid extremely large messages
        if text.len() > 100_000 {
            return Err("Message too large".into());
        }

        // Parse depth update data
        let depth: serde_json::Value = serde_json::from_str(text)?;

        // Binance depth stream format:
        // - Snapshot (REST): { "lastUpdateId": ..., "bids": [[price, qty], ...], "asks": [[price, qty], ...] }
        // - WS updates (like btcusdt@depth20@100ms):
        //   { "e": "depthUpdate", "E": ..., "b": [[price, qty], ...], "a": [[price, qty], ...], ... }
        let event_type = depth.get("e").and_then(|e| e.as_str());
        let is_snapshot = depth.get("lastUpdateId").is_some();
        let is_update = event_type == Some("depthUpdate");

        // Only process depth snapshots and updates
        if !is_snapshot && !is_update {
            return Ok(());
        }

        let exchange_timestamp = depth
            .get("E")
            .and_then(|e| e.as_u64())
            .unwrap_or(0);

        // Process bids (buy side). Prefer WS keys "b", fall back to "bids".
        if let Some(bids) = depth
            .get("b")
            .or_else(|| depth.get("bids"))
            .and_then(|b| b.as_array())
        {
            for bid in bids {
                if let Some(bid_array) = bid.as_array() {
                    if bid_array.len() >= 2 {
                        if let (Some(price_str), Some(qty_str)) =
                            (bid_array[0].as_str(), bid_array[1].as_str())
                        {
                            let price_opt = parse_price_cents(price_str);
                            let quantity_opt = parse_quantity_smallest_unit(qty_str, 8); // BTC has 8 decimals

                            if let (Some(price), Some(quantity)) = (price_opt, quantity_opt) {
                                let _ = self
                                    .tx
                                    .send(ExchangePrice::Binance {
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

        // Process asks (sell side). Prefer WS keys "a", fall back to "asks".
        if let Some(asks) = depth
            .get("a")
            .or_else(|| depth.get("asks"))
            .and_then(|a| a.as_array())
        {
            for ask in asks {
                if let Some(ask_array) = ask.as_array() {
                    if ask_array.len() >= 2 {
                        if let (Some(price_str), Some(qty_str)) =
                            (ask_array[0].as_str(), ask_array[1].as_str())
                        {
                            let price_opt = parse_price_cents(price_str);
                            let quantity_opt = parse_quantity_smallest_unit(qty_str, 8);

                            if let (Some(price), Some(quantity)) = (price_opt, quantity_opt) {
                                let _ = self
                                    .tx
                                    .send(ExchangePrice::Binance {
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

