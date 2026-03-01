use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio_stream::{wrappers::IntervalStream, Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::orderbook::OrderBook;

pub mod pb {
    tonic::include_proto!("orderbook");
}

use pb::{
    orderbook_aggregator_server::{OrderbookAggregator, OrderbookAggregatorServer},
    Empty, Level, Summary,
};

pub struct OrderbookService {
    pub orderbook: Arc<OrderBook>,
}

type SummaryStream =
    Pin<Box<dyn Stream<Item = Result<Summary, Status>> + Send + Sync + 'static>>;

#[tonic::async_trait]
impl OrderbookAggregator for OrderbookService {
    type BookSummaryStream = SummaryStream;

    async fn book_summary(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::BookSummaryStream>, Status> {
        let ob = self.orderbook.clone();

        // Stream a snapshot every 500ms.
        let interval = tokio::time::interval(Duration::from_millis(500));
        let stream = IntervalStream::new(interval).map(move |_| {
            let _span = tracing::info_span!("grpc_snapshot").entered();
            let top_bids = {
                let _s = tracing::info_span!("top_bids").entered();
                ob.top_bids_all_exchanges()
            };
            let top_asks = {
                let _s = tracing::info_span!("top_asks").entered();
                ob.top_asks_all_exchanges()
            };
            let spread_cents = ob.spread_all_exchanges();

            let (bids, asks, spread) = {
                let _s = tracing::info_span!("build_proto").entered();
                let bids: Vec<Level> = top_bids
                .iter()
                .map(|(exchange, price_cents, qty_smallest)| {
                    let exchange_str = match exchange {
                        crate::api::Exchange::Binance => "binance",
                        crate::api::Exchange::Bitstamp => "bitstamp",
                    }
                    .to_string();

                    Level {
                        exchange: exchange_str,
                        price: *price_cents as f64 / 100.0,
                        amount: *qty_smallest as f64 / 1e8, // assuming 8 decimals
                    }
                })
                .collect();

            let asks: Vec<Level> = top_asks
                .iter()
                .map(|(exchange, price_cents, qty_smallest)| {
                    let exchange_str = match exchange {
                        crate::api::Exchange::Binance => "binance",
                        crate::api::Exchange::Bitstamp => "bitstamp",
                    }
                    .to_string();

                    Level {
                        exchange: exchange_str,
                        price: *price_cents as f64 / 100.0,
                        amount: *qty_smallest as f64 / 1e8,
                    }
                })
                .collect();

                let spread = spread_cents.map(|c| c as f64 / 100.0).unwrap_or(0.0);

                (bids, asks, spread)
            };

            Ok(Summary {
                spread,
                bids,
                asks,
            })
        });

        Ok(Response::new(Box::pin(stream) as Self::BookSummaryStream))
    }
}

pub async fn run_grpc_server(orderbook: Arc<OrderBook>) -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse()?;
    let service = OrderbookService { orderbook };

    tonic::transport::Server::builder()
        .add_service(OrderbookAggregatorServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

