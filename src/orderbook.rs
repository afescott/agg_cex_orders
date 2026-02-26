use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use dashmap::DashMap;

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

                println!(
                    "Adding ask price level: exchange={:?}, price={}, quantity={}",
                    exchange, price, quantity
                );

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

    /// Top 10 bid levels from the combined book (price, quantity), sorted best-first.
    pub fn top_bids_all_exchanges(&self) -> Vec<(u64, u64)> {
        let mut combined: BTreeMap<u64, u64> = BTreeMap::new();

        // Aggregate all bids from all exchanges into a single price -> total quantity map.
        for entry in self.exchange_bids_price_level.iter() {
            let map_arc = entry.value();
            if let Ok(guard) = map_arc.read() {
                for (&price, &qty) in guard.iter() {
                    if qty == 0 {
                        continue;
                    }
                    *combined.entry(price).or_insert(0) += qty;
                }
            }
        }

        // Take up to 10 by price (highest first).
        combined
            .iter()
            .rev()
            .take(10)
            .map(|(&price, &qty)| (price, qty))
            .collect()
    }

    /// Top 10 ask levels from the combined book (price, quantity), sorted best-first.
    pub fn top_asks_all_exchanges(&self) -> Vec<(u64, u64)> {
        let mut combined: BTreeMap<u64, u64> = BTreeMap::new();

        // Aggregate all asks from all exchanges into a single price -> total quantity map.
        for entry in self.exchange_asks_price_level.iter() {
            let map_arc = entry.value();
            if let Ok(guard) = map_arc.read() {
                for (&price, &qty) in guard.iter() {
                    if qty == 0 {
                        continue;
                    }
                    *combined.entry(price).or_insert(0) += qty;
                }
            }
        }

        // Take up to 10 by price (lowest first for asks).
        combined
            .iter()
            .take(10)
            .map(|(&price, &qty)| (price, qty))
            .collect()
    }

    /// Spread across all exchanges: best ask price - best bid price (in cents)
    /// using the combined top-of-book from all exchanges.
    /// Returns `None` if either side is missing or the book is crossed.
    pub fn spread_all_exchanges(&self) -> Option<u64> {
        let top_bids = self.top_bids_all_exchanges();
        let top_asks = self.top_asks_all_exchanges();

        let (best_bid_price, _) = top_bids.first().copied()?;
        let (best_ask_price, _) = top_asks.first().copied()?;

        if best_ask_price > best_bid_price {
            Some(best_ask_price - best_bid_price)
        } else {
            None
        }
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
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0], (100, 3));
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
        assert_eq!(bids[0].0, 104);
        assert_eq!(bids[4].0, 100);
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
        assert_eq!(asks[0].0, 200);
        assert_eq!(asks[2].0, 220);
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
