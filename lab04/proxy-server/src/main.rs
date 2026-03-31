use hyper::{
    Request, Response,
    body::{Bytes, Frame, Incoming},
    server::conn::http1,
    service::service_fn,
};
use http_body_util::{
    BodyExt, Empty, Full, StreamBody,
    combinators::BoxBody,
};
use hyper_util::rt::TokioIo;
use futures_util::stream::StreamExt;
use log::{info, warn};
use tokio::{
    net::TcpListener,
    sync::Mutex,
};

use std::{
    collections::HashSet,
    env,
    net::SocketAddr, sync::Arc,
};

struct State {
    journal: Vec<(String, String)>,
    cache: HashSet<String>,
    blacklist: HashSet<String>,
}

impl State {
    fn new(blacklist: Vec<String>) -> Self {
        Self {
            journal: Vec::default(),
            cache: HashSet::default(),
            blacklist: HashSet::from_iter(blacklist),
        }
    }

    fn add_request(&mut self, uri: http::Uri, status: http::StatusCode) {
        self.journal.push((uri.to_string(), status.to_string()));
    }

    fn is_allowed(&self, uri: &http::Uri) -> bool {
        let path = uri.path();
        let host = path.split('/').skip(1).collect::<Vec<_>>()[0];
        !self.blacklist.contains(host)
    }
}

async fn accept(journal: Arc<Mutex<State>>, req: Request<Incoming>) -> Result<Response<BoxBody<Bytes, reqwest::Error>>, anyhow::Error> {  
    let (mut parts, body) = req.into_parts();
    parts.headers.remove("host");

    // !("{:?}", parts.headers);

    let uri = &parts.uri;
    info!("serving request to {}", parts.uri);

    if !journal.lock().await.is_allowed(uri) {
        warn!("request to blacklisted host, responding 403");
        return Ok(http::Response::builder()
            .status(403)
            .body(BoxBody::new(
                Full::new(Bytes::from_static(&b"host is blacklisted!"[..])).map_err(|inf| match inf {})
            ))?)
    }

    if uri.path() == "/favicon.ico" {
        warn!("request to favicon, responding 404");
        return Ok(http::Response::builder()
            .status(404)
            .body(
                BoxBody::<_, reqwest::Error>::new(
                    Empty::new().map_err(|inf| match inf {})
                ))?);
    }

    let url = format!("http:/{}", parts.uri);
    
    let client = reqwest::Client::new();

    let resp = client
        .request(parts.method, url)
        .headers(parts.headers)
        .body(body.collect().await.unwrap().to_bytes())
        .send()
        .await?;
    let status = resp.status();
    
    info!("status: {}", resp.status());

    journal.lock().await.add_request(uri.clone(), status);

    let mut builder = http::response::Builder::new()
        .status(resp.status());
    builder = resp
        .headers()
        .iter()
        .fold(builder, |b, (h, v)| b.header(h, v));
    Ok(builder
        .body(BoxBody::new(StreamBody::new(
            resp.bytes_stream().map(|bytes|
                bytes.map(|bts| Frame::data(bts))
            )
        )))?)
}

#[tokio::main]  
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();
    let args = env::args().collect::<Vec<_>>();
    let addr = SocketAddr::from(
        ([127, 0, 0, 1], args[1].parse::<u16>().expect("incorrect port"))
    );
    let journal = Arc::new(Mutex::new(
        State::new(vec!["www.southcn.com".to_owned()])
    ));
    let listener = TcpListener::bind(addr).await?;  
  
    loop {  
        let (stream, _) = listener.accept().await?;  
  
        let io = TokioIo::new(stream);  
  
        let jrnl = journal.clone();
        tokio::task::spawn(async move {  
            if let Err(err) = http1::Builder::new()  
                .serve_connection(io, service_fn(move |req| 
                    accept(jrnl.clone(), req)
                ))  
                .await {  
                eprintln!("Error serving connection: {:?}", err);  
            }  
        });  
    }  
}