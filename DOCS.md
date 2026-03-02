## Overview

This service connects to two exchange WebSocket feeds (Binance and Bitstamp), keeps an in‑memory view of their order books for a single trading pair, and exposes a gRPC stream of the **combined** top of book:

- Top 10 bids and asks across both venues
- Per level: which exchange, price, and quantity
- Current spread (best ask – best bid)

## High‑level architecture

- **`main`**
  - Reads `TRADING_PAIR` (with a sensible default).
  - Creates a shared `OrderBook` and an `mpsc` channel for `ExchangePrice` updates.
  - Spawns:
    - gRPC server (`api::grpc::run_grpc_server`)
    - Binance WebSocket client (`api::binance::BinanceClient::listen_pair`)
    - Bitstamp WebSocket client (`api::bitstamp::BitstampClient::listen_pair`)
  - Listens on the channel and applies every `ExchangePrice` to the order book.

- **Exchange clients (`api::binance`, `api::bitstamp`)**
  - Maintain a single WebSocket connection per exchange.
  - For each inbound message:
    - Parse JSON into an exchange‑specific shape.
    - Convert price/size into:
      - **price in cents** (u64)
      - **quantity in base units** (e.g. satoshis) via `util::parse_quantity_smallest_unit`.
    - Send an `ExchangePrice` enum over the `mpsc` channel.

- **Order book (`orderbook`)**
  - Per‑exchange price levels stored as `DashMap<Exchange, Arc<RwLock<BTreeMap<u64, u64>>>>`.
  - `update_price_level` maintains per‑venue maps.
  - `top_bids_all_exchanges` / `top_asks_all_exchanges`:
    - Flatten all venues into a single sorted list.
    - Return up to 10 best levels (descending for bids, ascending for asks).
  - `spread_all_exchanges`:
    - Uses the best bid and best ask from the combined view.

- **gRPC API (`api::grpc`)**
  - `OrderbookAggregator/BookSummary`:
    - Streams a `Summary` snapshot every 500ms.
    - Each snapshot is derived from the current `OrderBook` in memory.

## Observability

- **tokio‑console**
  - Enabled via `console-subscriber` with Tokio’s tracing features.
  - Run the app, then in another terminal:
    - `tokio-console` to see:
      - per‑task busy/idle time
      - how often each task is polled
      - which tasks are blocked on I/O vs CPU.

- **Tracing spans**
  - The code instruments key steps such as:
    - `handle_message` (Binance/Bitstamp) with nested spans:
      - `parse_json`
      - `process_bids`
      - `process_asks`
    - `grpc_snapshot` with nested spans:
      - `top_bids`
      - `top_asks`
      - `build_proto`
    - `update_book` / `update_price_level` / `write_level`
  - These are visible both in logs (via `RUST_LOG`) and in tokio‑console.


