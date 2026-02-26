mod binance;
mod coinbase;
mod grpc;

pub enum Exchange {
    Binance,
    Coinbase,
}

pub enum Side {
    Buy,
    Sell,
}

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
