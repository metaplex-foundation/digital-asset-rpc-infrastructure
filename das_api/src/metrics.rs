use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use std::{net::SocketAddr, sync::Once};

use crate::error::DasApiError;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
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

pub fn run_server(address: SocketAddr) -> anyhow::Result<()> {
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

    let make_service = make_service_fn(move |_: &AddrStream| async move {
        Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| async move {
            let response = match req.uri().path() {
                "/metrics" => metrics_handler(),
                _ => not_found_handler(),
            };
            Ok::<_, hyper::Error>(response)
        }))
    });

    let server = Server::try_bind(&address)?.serve(make_service);
    info!("prometheus server started: http://{address:?}/metrics");

    tokio::spawn(async move {
        if let Err(error) = server.await {
            error!("prometheus server failed: {error:?}");
        }
    });

    Ok(())
}

fn metrics_handler() -> Response<Body> {
    let metrics = TextEncoder::new()
        .encode_to_string(&REGISTRY.gather())
        .unwrap_or_else(|error| {
            error!("could not encode custom metrics: {}", error);
            String::new()
        });
    Response::builder()
        .header("content-type", "text/plain")
        .body(Body::from(metrics))
        .unwrap()
}

fn not_found_handler() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
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
