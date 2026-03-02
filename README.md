Run the aggregator (optional `TRADING_PAIR`, e.g. `ETH/USDT`):

```bash
TRADING_PAIR=BTC-USDT cargo run
```

Stream the gRPC order book summaries:

```bash
./grpcurl -plaintext \
  -format json -emit-defaults \
  -import-path . \
  -proto proto/orderbook.proto \
  0.0.0.0:50051 \
  orderbook.OrderbookAggregator/BookSummary
```

Flamegraph for span-based monitoring

```bash
cargo flamegraph           
```

For live task‑level insight, you can also run `tokio-console` in another terminal while the app is running to see which tasks are busy, idle, or blocked on I/O.


