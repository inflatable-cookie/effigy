use super::*;
use crate::routes::{Route, RouteSource};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Abort(tokio::task::JoinHandle<()>);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn upstream(label: &'static str, close_first: bool) -> (SocketAddr, Arc<AtomicUsize>, Abort) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let connections = count.clone();
    let task = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            stream.set_nodelay(true).unwrap();
            let number = connections.fetch_add(1, Ordering::Relaxed);
            if close_first && number == 0 {
                drop(stream);
                continue;
            }
            clients.spawn(async move {
                let service = service_fn(move |req: Request<Incoming>| async move {
                    let close = req.uri().path() == "/close";
                    let body = if req.method() == hyper::Method::POST {
                        incoming_body(req.into_body())
                    } else {
                        full_body(Bytes::from_static(label.as_bytes()))
                    };
                    let mut response = Response::new(body);
                    if close {
                        response
                            .headers_mut()
                            .insert(CONNECTION, HeaderValue::from_static("close"));
                    }
                    Ok::<_, hyper::Error>(response)
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
            while clients.try_join_next().is_some() {}
        }
    });
    (address, count, Abort(task))
}
fn route(target: SocketAddr) -> Route {
    Route {
        domain: "pool.test".into(),
        target: Some(target.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Task,
        project: "/fixture".into(),
        managed_listener: None,
        tls: false,
        registered: chrono::Utc::now(),
    }
}
async fn proxy(
    route: Route,
) -> (
    SocketAddr,
    Arc<RwLock<RouteTable>>,
    tokio::sync::watch::Sender<bool>,
    tokio::task::JoinHandle<Result<(), crate::GatewayError>>,
) {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    crate::locks::write_tolerant(&table).upsert(route);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let server = tokio::spawn(run_proxy_server_on(
        listener,
        ProxyConfig::default(),
        table.clone(),
        Arc::new(GatewayStats::new()),
        shutdown,
    ));
    (address, table, stop, server)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pool_honours_close_streaming_concurrency_and_target_changes() {
    let (first, count, _first) = upstream("first", false).await;
    let (second, _, _second) = upstream("second", false).await;
    let (address, table, stop, server) = proxy(route(first)).await;
    let client = client();
    let url = format!("http://{address}");
    for _ in 0..3 {
        let response = client
            .get(format!("{url}/close"))
            .header(HOST, "pool.test")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "first");
    }
    assert_eq!(
        count.load(Ordering::Relaxed),
        3,
        "upstream Connection: close must end reuse"
    );
    let mut requests = tokio::task::JoinSet::new();
    for index in 0..8u8 {
        let client = client.clone();
        let url = url.clone();
        requests.spawn(async move {
            let payload = vec![index; 1024 * 1024];
            let response = client
                .post(&url)
                .header(HOST, "pool.test")
                .body(payload.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.bytes().await.unwrap().as_ref(), payload.as_slice());
        });
    }
    while let Some(result) = requests.join_next().await {
        result.unwrap();
    }
    crate::locks::write_tolerant(&table).upsert(route(second));
    let response = client
        .get(&url)
        .header(HOST, "pool.test")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.text().await.unwrap(),
        "second",
        "old target pool must not survive a route change"
    );
    drop(client);
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
}
