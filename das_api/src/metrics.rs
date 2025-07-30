use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use std::{net::SocketAddr, sync::Once};

use crate::error::DasApiError;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto;

use tokio::net::TcpListener;

use pin_project::pin_project;
use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder};
use tracing::{error, info};

lazy_static::lazy_static! {
    static ref REGISTRY: Registry = Registry::new();

    static ref DAS_API_REQUESTS_TOTAL:IntCounterVec = IntCounterVec::new(
        Opts::new("das_api_requests_total", "Total number of DAS API calls, labelled by method and status"),
        &["method", "status"]
    ).unwrap();

    pub static ref DAS_API_REQUEST_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "das_api_request_duration_seconds",
            "API request latency in seconds, labeled by method."
        )
        .buckets(vec![
            0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500,
            1.000, 2.500, 5.000, 10.000, 30.000, 60.000
        ]),
        &["method"],
    )
    .unwrap();
}

fn metrics_handler() -> Result<Response<Full<Bytes>>, Infallible> {
    let metrics = TextEncoder::new()
        .encode_to_string(&REGISTRY.gather())
        .unwrap_or_else(|error| {
            error!("could not encode custom metrics: {error}");
            String::new()
        });

    Ok(Response::builder()
        .header("content-type", "text/plain")
        .body(Full::new(Bytes::from(metrics)))
        .unwrap())
}

async fn handle_metrics_request(
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    match req.uri().path() {
        "/metrics" => metrics_handler(),
        _ => Ok(not_found_handler()),
    }
}

fn not_found_handler() -> Response<Full<Bytes>> {
    Response::builder()
        .status(404)
        .body(Full::new(Bytes::from("Not Found")))
        .unwrap()
}

pub fn run_metrics_server(address: SocketAddr) -> anyhow::Result<()> {
    // Register once
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        macro_rules! register {
            ($collector:ident) => {
                REGISTRY
                    .register(Box::new($collector.clone()))
                    .expect("collector can't be registered");
            };
        }
        register!(DAS_API_REQUESTS_TOTAL);
        register!(DAS_API_REQUEST_DURATION_SECONDS);
    });

    tokio::spawn(async move {
        let listener = match TcpListener::bind(address).await {
            Ok(l) => {
                info!("Prometheus server started at http://{address}/metrics");
                l
            }
            Err(e) => {
                error!("Failed to bind Prometheus server: {e:?}");
                return;
            }
        };

        loop {
            let (stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Prometheus accept failed: {e:?}");
                    continue;
                }
            };

            let io = TokioIo::new(stream);
            let service = service_fn(move |req: Request<Incoming>| handle_metrics_request(req));

            tokio::spawn(async move {
                let builder = auto::Builder::new(hyper_util::rt::TokioExecutor::new());
                let conn = builder.serve_connection(io, service);
                if let Err(e) = conn.await {
                    error!("Prometheus connection failed: {e:?}");
                }
            });
        }
    });

    Ok(())
}

#[derive(Debug, Clone)]
#[repr(u8)]
pub enum DasApiMethod {
    CheckHealth,
    GetSlot,
    GetAssetProof,
    GetAssetProofs,
    GetAsset,
    GetAssets,
    GetAssetsByOwner,
    GetAssetsByGroup,
    GetAssetsByCreator,
    GetAssetsByAuthority,
    SearchAssets,
    GetAssetSignatures,
    GetTokenAccounts,
    GetNftEditions,
    GetTokenLargestAccounts,
    GetTokenSupply,
    GetTokenAccountsByOwner,
    GetTokenAccountsByDelegate,
}

impl DasApiMethod {
    pub const fn as_str(&self) -> &str {
        match self {
            DasApiMethod::CheckHealth => "checkHealth",
            DasApiMethod::GetSlot => "getSlot",
            DasApiMethod::GetAssetProof => "getAssetProof",
            DasApiMethod::GetAssetProofs => "getAssetProofs",
            DasApiMethod::GetAsset => "getAsset",
            DasApiMethod::GetAssets => "getAssets",
            DasApiMethod::GetAssetsByOwner => "getAssetsByOwner",
            DasApiMethod::GetAssetsByGroup => "getAssetsByGroup",
            DasApiMethod::GetAssetsByCreator => "getAssetsByCreator",
            DasApiMethod::GetAssetsByAuthority => "getAssetsByAuthority",
            DasApiMethod::SearchAssets => "searchAssets",
            DasApiMethod::GetAssetSignatures => "getAssetSignatures",
            DasApiMethod::GetTokenAccounts => "getTokenAccounts",
            DasApiMethod::GetNftEditions => "getNftEditions",
            DasApiMethod::GetTokenLargestAccounts => "getTokenLargestAccounts",
            DasApiMethod::GetTokenSupply => "getTokenSupply",
            DasApiMethod::GetTokenAccountsByOwner => "getTokenAccountsByOwner",
            DasApiMethod::GetTokenAccountsByDelegate => "getTokenAccountsByDelegate",
        }
    }
}

pub enum ApiRequestStatus<'a> {
    Ok,
    Error(&'a DasApiError),
}

impl<'a> ApiRequestStatus<'a> {
    pub const fn as_str(&self) -> &str {
        match self {
            ApiRequestStatus::Ok => "OK",
            ApiRequestStatus::Error(e) => e.to_error_code(),
        }
    }
}

pub fn inc_das_api_status_total(method: &DasApiMethod, status: ApiRequestStatus) {
    DAS_API_REQUESTS_TOTAL
        .with_label_values(&[method.as_str(), status.as_str()])
        .inc();
}

pub fn record_das_api_latency(method: &DasApiMethod, time_elapsed: f64) {
    DAS_API_REQUEST_DURATION_SECONDS
        .with_label_values(&[method.as_str()])
        .observe(time_elapsed);
}

pub trait MetricsRecorderExt: Sized {
    fn record_metrics(self, method: DasApiMethod) -> MetricsRecorder<Self>;
}

#[pin_project]
pub struct MetricsRecorder<Fut> {
    method: DasApiMethod,
    #[pin]
    future: Fut,
    start: Instant,
}

impl<Fut, T> MetricsRecorderExt for Fut
where
    Fut: Future<Output = Result<T, DasApiError>> + Sized,
{
    fn record_metrics(self, method: DasApiMethod) -> MetricsRecorder<Fut> {
        MetricsRecorder {
            method,
            future: self,
            start: Instant::now(),
        }
    }
}

impl<Fut, T> Future for MetricsRecorder<Fut>
where
    Fut: Future<Output = Result<T, DasApiError>>,
{
    type Output = Result<T, DasApiError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let method = self.method.clone();
        let this = self.project();

        match this.future.poll(cx) {
            Poll::Ready(result) => {
                let elapsed = this.start.elapsed().as_secs_f64();
                record_das_api_latency(&method, elapsed);

                match &result {
                    Ok(_) => inc_das_api_status_total(&method, ApiRequestStatus::Ok),
                    Err(err) => {
                        inc_das_api_status_total(&method, ApiRequestStatus::Error(err));
                    }
                };
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
