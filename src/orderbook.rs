use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use dashmap::DashMap;

use crate::api::{Exchange, ExchangePrice, Side};

#[derive(Debug, Clone, Copy)]
pub struct BestPriceLevel {
    pub price: u64,
    pub quantity: u64,
}

pub struct OrderBook {
    /// The symbol or identifier for this order book
    pub symbol: String,
    // BTreeMap keeps prices sorted (bids: highest first, asks: lowest first) and maps price → quantity.
    pub exchange_bids_price_level: DashMap<Exchange, Arc<RwLock<BTreeMap<u64, u64>>>>,
    // One BTreeMap per exchange, sorted by price,
    pub exchange_asks_price_level: DashMap<Exchange, Arc<RwLock<BTreeMap<u64, u64>>>>,

    pub cached_best_bid: DashMap<Exchange, BestPriceLevel>,

    pub cached_best_ask: DashMap<Exchange, BestPriceLevel>,

    /// Best bid across all exchanges. Returns None if no data available.
    /// The tuple contains (exchange, price, quantity), where price of 0 means no data.
    pub best_bid_all_exchanges: Arc<std::sync::Mutex<(Exchange, BestPriceLevel)>>,

    /// Best ask across all exchanges. Returns None if no data available.
    /// The tuple contains (exchange, price, quantity), where price of 0 means no data.
    pub best_ask_all_exchanges: Arc<std::sync::Mutex<(Exchange, BestPriceLevel)>>,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        OrderBook {
            symbol,
            exchange_bids_price_level: DashMap::new(),
            exchange_asks_price_level: DashMap::new(),
            cached_best_bid: DashMap::new(),
            cached_best_ask: DashMap::new(),
            best_bid_all_exchanges: Arc::new(std::sync::Mutex::new((
                Exchange::Binance,
                BestPriceLevel {
                    price: 0,
                    quantity: 0,
                },
            ))),
            best_ask_all_exchanges: Arc::new(std::sync::Mutex::new((
                Exchange::Binance,
                BestPriceLevel {
                    price: 0,
                    quantity: 0,
                },
            ))),
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

                // Update cached best bid if this is the new best (highest price for bids)
                let best_bid = guard.keys().next_back().copied(); // BTreeMap is sorted, last is highest
                if let Some(best_price) = best_bid {
                    let best_quantity = guard.get(&best_price).copied().unwrap_or(0);
                    drop(guard); // Explicitly drop guard before calling update
                    self.update_cached_best_bid(exchange, best_price, best_quantity);
                }
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

                // Update cached best ask if this is the new best (lowest price for asks)
                let best_ask = guard.keys().next().copied(); // BTreeMap is sorted, first is lowest
                if let Some(best_price) = best_ask {
                    let best_quantity = guard.get(&best_price).copied().unwrap_or(0);
                    drop(guard); // Explicitly drop guard before calling update
                    self.update_cached_best_ask(exchange, best_price, best_quantity);
                }
            }
        }
    }

    fn update_cached_best_bid(&self, exchange: Exchange, price: u64, quantity: u64) {
        let level = BestPriceLevel { price, quantity };
        self.cached_best_bid.insert(exchange, level);

        let mut best = self.best_bid_all_exchanges.lock().unwrap();
        if level.price > best.1.price {
            *best = (exchange, level);
        }
    }

    fn update_cached_best_ask(&self, exchange: Exchange, price: u64, quantity: u64) {
        let level = BestPriceLevel { price, quantity };
        self.cached_best_ask.insert(exchange, level);

        let mut best = self.best_ask_all_exchanges.lock().unwrap();
        if best.1.price == 0 || level.price < best.1.price {
            *best = (exchange, level);
        }
    }
}
