use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use dashmap::DashMap;
use serde_json::json;

use crate::api::{Exchange, ExchangePrice, Side};

pub struct OrderBook {
    /// The symbol or identifier for this order book
    pub symbol: String,
    // BTreeMap keeps prices sorted (bids: highest first, asks: lowest first) and maps price → quantity.
    pub exchange_bids_price_level: DashMap<Exchange, Arc<RwLock<BTreeMap<u64, u64>>>>,
    // One BTreeMap per exchange, sorted by price,
    pub exchange_asks_price_level: DashMap<Exchange, Arc<RwLock<BTreeMap<u64, u64>>>>,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        OrderBook {
            symbol,
            exchange_bids_price_level: DashMap::new(),
            exchange_asks_price_level: DashMap::new(),
        }
    }

    /// Update the per-exchange price levels from a single exchange-level price update.
    pub fn update_price_level(&self, order: ExchangePrice) {
        match order {
            ExchangePrice::Binance {
                price,
                quantity,
                side,
                ..
            } => {
                self.update_price_level_for_exchange(Exchange::Binance, price, quantity, side);
            }
            ExchangePrice::Bitstamp {
                price,
                quantity,
                side,
                ..
            } => {
                self.update_price_level_for_exchange(Exchange::Bitstamp, price, quantity, side);
            }
        }
    }

    fn update_price_level_for_exchange(
        &self,
        exchange: Exchange,
        price: u64,
        quantity: u64,
        side: Side,
    ) {
        match side {
            Side::Buy => {
                let price_level = self
                    .exchange_bids_price_level
                    .entry(exchange)
                    .or_insert_with(|| Arc::new(RwLock::new(BTreeMap::new())));
                let mut guard = match (*price_level.value()).write() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let entry = guard.entry(price).or_insert(0);
                *entry += quantity;

                // We can compute best bid on demand later by inspecting this BTreeMap.
            }
            Side::Sell => {
                let price_level = self
                    .exchange_asks_price_level
                    .entry(exchange)
                    .or_insert_with(|| Arc::new(RwLock::new(BTreeMap::new())));
                let mut guard = match (*price_level.value()).write() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let entry = guard.entry(price).or_insert(0);
                *entry += quantity;

                // We can compute best ask on demand later by inspecting this BTreeMap.
            }
        }
    }

    /// Top 10 bid levels from the combined book (exchange, price, quantity), sorted best-first.
    pub fn top_bids_all_exchanges(&self) -> Vec<(Exchange, u64, u64)> {
        let mut levels: Vec<(Exchange, u64, u64)> = Vec::new();

        // Collect all bid levels from all exchanges.
        for entry in self.exchange_bids_price_level.iter() {
            let exchange = *entry.key();
            let map_arc = entry.value();
            if let Ok(guard) = map_arc.read() {
                for (&price, &qty) in guard.iter() {
                    if qty == 0 {
                        continue;
                    }
                    levels.push((exchange, price, qty));
                }
            }
        }

        // Sort by price descending and take up to 10.
        levels.sort_by(|a, b| b.1.cmp(&a.1));
        if levels.len() > 10 {
            levels.truncate(10);
        }
        levels
    }

    /// Top 10 ask levels from the combined book (exchange, price, quantity), sorted best-first.
    pub fn top_asks_all_exchanges(&self) -> Vec<(Exchange, u64, u64)> {
        let mut levels: Vec<(Exchange, u64, u64)> = Vec::new();

        // Collect all ask levels from all exchanges.
        for entry in self.exchange_asks_price_level.iter() {
            let exchange = *entry.key();
            let map_arc = entry.value();
            if let Ok(guard) = map_arc.read() {
                for (&price, &qty) in guard.iter() {
                    if qty == 0 {
                        continue;
                    }
                    levels.push((exchange, price, qty));
                }
            }
        }

        // Sort by price ascending and take up to 10.
        levels.sort_by(|a, b| a.1.cmp(&b.1));
        if levels.len() > 10 {
            levels.truncate(10);
        }
        levels
    }

    /// Spread across all exchanges: best ask price - best bid price (in cents)
    /// using the combined top-of-book from all exchanges.
    /// Returns `None` only if either side is missing.
    pub fn spread_all_exchanges(&self) -> Option<u64> {
        let top_bids = self.top_bids_all_exchanges();
        let top_asks = self.top_asks_all_exchanges();

        let (_, best_bid_price, _) = top_bids.first().copied()?;
        let (_, best_ask_price, _) = top_asks.first().copied()?;

        // Always return a numeric spread when both sides exist, even if crossed/locked.
        Some(best_ask_price.saturating_sub(best_bid_price))
    }
}

impl OrderBook {
    /// Print a JSON summary of the current combined book: spread, top 10 bids, top 10 asks.
    pub fn print_snapshot_json(&self) {
        let top_bids = self.top_bids_all_exchanges();
        let top_asks = self.top_asks_all_exchanges();
        let spread_cents = self.spread_all_exchanges();

        let bids_json: Vec<_> = top_bids
            .into_iter()
            .map(|(exchange, price_cents, qty_smallest)| {
                let exchange_str = match exchange {
                    Exchange::Binance => "binance",
                    Exchange::Bitstamp => "bitstamp",
                };
                json!({
                    "exchange": exchange_str,
                    "price": price_cents as f64 / 100.0,
                    "amount": qty_smallest as f64 / 1e8,
                })
            })
            .collect();

        let asks_json: Vec<_> = top_asks
            .into_iter()
            .map(|(exchange, price_cents, qty_smallest)| {
                let exchange_str = match exchange {
                    Exchange::Binance => "binance",
                    Exchange::Bitstamp => "bitstamp",
                };
                json!({
                    "exchange": exchange_str,
                    "price": price_cents as f64 / 100.0,
                    "amount": qty_smallest as f64 / 1e8,
                })
            })
            .collect();

        let snapshot = json!({
            "spread": spread_cents.map(|c| c as f64 / 100.0),
            "asks": asks_json,
            "bids": bids_json,
        });

        println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ExchangePrice, Side};

    fn ob() -> OrderBook {
        OrderBook::new("TEST".to_string())
    }

    #[test]
    fn aggregates_bids_across_exchanges() {
        let ob = ob();

        ob.update_price_level(ExchangePrice::Binance {
            price: 100,
            quantity: 1,
            exchange_timestamp: 0,
            received_at: 0,
            side: Side::Buy,
        });
        ob.update_price_level(ExchangePrice::Bitstamp {
            price: 100,
            quantity: 2,
            exchange_timestamp: 0,
            received_at: 0,
            side: Side::Buy,
        });

        let bids = ob.top_bids_all_exchanges();
        assert_eq!(bids.len(), 2);
        let total_qty: u64 = bids
            .iter()
            .filter(|(_, price, _)| *price == 100)
            .map(|(_, _, qty)| *qty)
            .sum();
        assert_eq!(total_qty, 3);
    }

    #[test]
    fn respects_less_than_ten_levels() {
        let ob = ob();

        // Insert 5 distinct bid levels
        for i in 0..5 {
            ob.update_price_level(ExchangePrice::Binance {
                price: 100 + i,
                quantity: 1,
                exchange_timestamp: 0,
                received_at: 0,
                side: Side::Buy,
            });
        }

        let bids = ob.top_bids_all_exchanges();
        assert_eq!(bids.len(), 5);
        // Highest price first
        assert_eq!(bids[0].1, 104);
        assert_eq!(bids[4].1, 100);
    }

    #[test]
    fn top_asks_sorted_lowest_first() {
        let ob = ob();

        for i in 0..3 {
            ob.update_price_level(ExchangePrice::Binance {
                price: 200 + i * 10,
                quantity: 1,
                exchange_timestamp: 0,
                received_at: 0,
                side: Side::Sell,
            });
        }

        let asks = ob.top_asks_all_exchanges();
        assert_eq!(asks.len(), 3);
        assert_eq!(asks[0].1, 200);
        assert_eq!(asks[2].1, 220);
    }

    #[test]
    fn spread_computed_from_top_of_book() {
        let ob = ob();

        // Best bid: 100, best ask: 110
        ob.update_price_level(ExchangePrice::Binance {
            price: 100,
            quantity: 1,
            exchange_timestamp: 0,
            received_at: 0,
            side: Side::Buy,
        });
        ob.update_price_level(ExchangePrice::Binance {
            price: 110,
            quantity: 1,
            exchange_timestamp: 0,
            received_at: 0,
            side: Side::Sell,
        });

        let spread = ob.spread_all_exchanges();
        assert_eq!(spread, Some(10));
    }
}
