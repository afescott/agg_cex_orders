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

