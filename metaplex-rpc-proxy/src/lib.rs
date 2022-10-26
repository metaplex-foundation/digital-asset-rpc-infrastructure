use std::env;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use std::time::Duration;
use lazy_static::lazy_static;
use regex::Regex;
use log::info;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_, _| -> Box<dyn HttpContext> { Box::new(Root) });
}}

#[derive(Debug)]
struct Root;

impl Context for Root {}

impl RootContext for Root {
    fn create_http_context(&self, _context_id: u32) -> Option<Box<dyn HttpContext>> {
        let config = self.get_vm_configuration();
        Some(Box::new(RpcProxy::new(config)))
    }
}


#[derive(Debug)]
struct RpcProxy{
    config: Option<Bytes>,
};

impl RpcProxy {
    fn new(config: Option<Bytes>) -> Self {

        return Self {
            config,
        };
    }
}

fn call(service: &'static str, proxy: &mut RpcProxy, body: Bytes) -> Result<u32, Status> {
    let path = env::var("PROXY_AUTH").unwrap_or("".to_string());
    proxy.dispatch_http_call(
        service,
        vec![
            (":method", "POST"),
            (":path", format!("/{}", path).as_str()),
            (":authority", service),
            ("content-type", "application/json"),
            ("content-length", &body.len().to_string()),
        ],
        Some(&*body),
        vec![],
        Duration::from_secs(300),
    )
}

fn upstream_rpc_call(proxy: &mut RpcProxy, body: Bytes) -> Result<u32, Status> {
    call("rpc", proxy, body)
}

impl Context for RpcProxy {
    fn on_http_call_response(&mut self, _token_id: u32, _num_headers: usize, body_size: usize, _num_trailers: usize) {
        info!("Response READ API: {}", body_size);
        let headers = self.get_http_call_response_headers();
        let static_headers: Vec<(&str, &str)> = headers.iter().map(|(s, v)| (s.as_str(), v.as_str())).collect();
        info!("Response READ API: {:?}", static_headers);
        if let Some(resp_body) = self.get_http_call_response_body(0, body_size) {
            info!("Response READ API");
            self.send_http_response(200, static_headers, Some(&*resp_body));
        }
    }
}

impl HttpContext for RpcProxy {
    fn on_http_request_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        lazy_static! {
            static ref FILTER: Regex = Regex::new(r"asset").unwrap();
        }
        if !end_of_stream {
            return Action::Pause;
        }
        if let Some(body) = self.get_http_request_body(0, body_size) {
            if let Ok(body_str) = String::from_utf8(body.clone()) {
                let read_api = FILTER.is_match(&body_str);
                info!("Read API: {} {}", read_api, body_str);
                if read_api {
                    return Action::Continue;
                } else {
                    let res = upstream_rpc_call(self, body);
                    return match res {
                        Ok(res) => Action::Pause,
                        Err(e) => {
                            info!("Error: {:?}", e);
                            Action::Continue
                        }
                    };
                }
            }
        }
        Action::Continue
    }
}
