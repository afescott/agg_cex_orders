pub mod binance;
pub mod coinbase;
// pub mod grpc;

#[derive(Debug, Clone, Copy)]
pub enum Exchange {
    Binance,
    Coinbase,
}

#[derive(Debug, Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

/// Logical trading pair shared across exchanges, configured at runtime.
///
/// Stored in a normalized "raw" string form (as provided via env),
/// and converted per-exchange as needed.
#[derive(Debug, Clone)]
pub struct TradingPair {
    raw: String,
}

impl TradingPair {
    /// Create from a string; returns `None` if empty/whitespace.
    pub fn from_str(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(TradingPair {
                raw: trimmed.to_string(),
            })
        }
    }

    /// Default trading pair when none is configured.
    pub fn default_pair() -> Self {
        // Use a common default; user can override via TRADING_PAIR env.
        TradingPair {
            raw: "BTC-USDT".to_string(),
        }
    }

    /// Human-readable form (as configured).
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Symbol used on Binance, e.g. "ETHUSDT", "SOLUSDT" (lowercased internally).
    pub fn binance_symbol(&self) -> String {
        // Drop separators and lowercase.
        self.raw
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | '/'))
            .collect::<String>()
            .to_ascii_lowercase()
    }

    /// Product ID used on Coinbase, e.g. "ETH-USD", "BTC-USD".
    pub fn coinbase_product_id(&self) -> String {
        self.raw
            .replace('_', "-")
            .replace('/', "-")
            .to_ascii_uppercase()
    }
}

#[derive(Debug)]
pub enum ExchangePrice {
    Binance {
        price: u64,              // Price in cents
        quantity: u64,           // Quantity in smallest unit (e.g., satoshis for BTC)
        exchange_timestamp: u64, // Timestamp from the exchange
        received_at: u64,        // Timestamp when we received the message
        side: Side,
    },
    Coinbase {
        price: u64,              // Price in cents
        quantity: u64,           // Quantity in smallest unit (e.g., satoshis for BTC)
        exchange_timestamp: u64, // Timestamp from the exchange
        received_at: u64,        // Timestamp when we received the message
        side: Side,
    },
}
