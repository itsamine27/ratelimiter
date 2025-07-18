#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::arbitrary_source_item_ordering)]
#![allow(clippy::question_mark_used)]
#![allow(clippy::str_to_string)]
#![allow(clippy::absolute_paths)]
#![allow(clippy::module_name_repetitions)]
mod error;
use crate::error::Result as MyRes;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    routing::get,
    serve, Router,
};
use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tower::{Layer, Service, ServiceBuilder};
use tracing::info;
#[tokio::main]
async fn main() -> MyRes<()> {
    //conf trancing
    tracing_subscriber::fmt::init();
    // Shared in‑memory counter map
    let state = Arc::new(Mutex::new(HashMap::new()));
    let app = Router::new()
        .route("/", get(hello_handler))
        .layer(ServiceBuilder::new().layer(RateLimitLayer { counter: state }));

    // Bind and run
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    info!("Listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn hello_handler() -> &'static str {
    "Hello, world!"
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone)]
struct RateLimitLayer {
    counter: Arc<Mutex<HashMap<String, usize>>>,
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitMiddleware<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimitMiddleware {
            inner,
            counter: self.counter.clone(),
            timer: Arc::new(AtomicU64::new(now_secs())),
        }
    }
}

#[derive(Clone)]
struct RateLimitMiddleware<S> {
    inner: S,
    counter: Arc<Mutex<HashMap<String, usize>>>,
    timer: Arc<AtomicU64>,
}

impl<S, B> Service<Request<B>> for RateLimitMiddleware<S>
where
    S: Service<Request<B>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<axum::BoxError> + Send,
    B: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Delegate readiness to the inner service
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let mut inner = self.inner.clone();
        let counter = self.counter.clone();
        let last_rest = self.timer.clone();
        Box::pin(async move {
            //reset timer after 10 minuts
            let cur = now_secs();
            let last = last_rest.load(Ordering::Relaxed);
            if cur.saturating_sub(last) >= 60 {
                let mut map = counter.lock().await;
                map.clear();
                last_rest.store(cur, Ordering::Relaxed);
                info!("🔄 Rate limit counters cleared after 1 minutes");
            }
            // Extract a “client IP” from x‑forwarded‑for (or default)
            let ip = req
                .headers()
                .get("x-forwarded-for")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unknown")
                .to_string();

            // Increment and check rate limit
            let mut map = counter.lock().await;
            let count = map.entry(ip.clone()).or_insert(0);
            *count += 1;

            if *count > 5 {
                // Too many requests
                let resp = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(Body::from("Too many requests"))
                    .unwrap();
                return Ok(resp);
            }

            // Otherwise forward to the inner handler
            inner.call(req).await
        })
    }
}
