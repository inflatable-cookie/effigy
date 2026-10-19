use super::*;
use crate::routes::{Route, RouteSource};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Abort(tokio::task::JoinHandle<()>);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn upstream(label: &'static str) -> (SocketAddr, Arc<AtomicUsize>, Abort) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let connections = count.clone();
    let task = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            stream.set_nodelay(true).unwrap();
            connections.fetch_add(1, Ordering::Relaxed);
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
        scope: None,
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
    let (first, count, _first) = upstream("first").await;
    let (second, _, _second) = upstream("second").await;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cached_managed_connection_refuses_revocation_and_changed_identity() {
    let fixture = tempfile::tempdir().unwrap();
    let (upstream, count, _upstream) = upstream("managed").await;
    let identity = crate::identity::read_live_process_identity(std::process::id()).unwrap();
    let availability = fixture.path().join("listener.json");
    let state = |status| serde_json::json!({"schema":"effigy.managed.host-listener-state.v1", "owner":"pool", "runtime_generation":"runtime", "generation":"child", "status":status});
    std::fs::write(&availability, state("ready").to_string()).unwrap();
    let owner = crate::routes::ManagedListenerRouteOwner {
        owner: "pool".into(),
        runtime_generation: "runtime".into(),
        generation: "child".into(),
        supervisor_pid: std::process::id(),
        supervisor_boot_identity: identity.boot_identity.clone(),
        supervisor_start_identity: identity.start_identity.clone(),
        root_pid: std::process::id(),
        root_boot_identity: identity.boot_identity.clone(),
        root_start_identity: identity.start_identity.clone(),
        listener_pid: std::process::id(),
        listener_boot_identity: identity.boot_identity,
        listener_start_identity: identity.start_identity,
        address: upstream.to_string(),
        route_tls: false,
        availability_file: availability.display().to_string(),
    };
    let mut managed = route(upstream);
    managed.managed_listener = Some(owner);
    let (address, table, stop, server) = proxy(managed.clone()).await;
    let client = client();
    let url = format!("http://{address}");
    let response = client
        .get(&url)
        .header(HOST, "pool.test")
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "managed");
    crate::locks::write_tolerant(&table).revision = 1;
    let response = client
        .get(&url)
        .header(HOST, "pool.test")
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "managed");
    assert!(
        count.load(Ordering::Relaxed) >= 2,
        "a new publication fence must prepare a new verified connection"
    );
    let connected = count.load(Ordering::Relaxed);
    std::fs::write(&availability, state("stopped").to_string()).unwrap();
    let response = client
        .get(&url)
        .header(HOST, "pool.test")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    std::fs::write(&availability, state("ready").to_string()).unwrap();
    managed
        .managed_listener
        .as_mut()
        .unwrap()
        .listener_boot_identity = "foreign-generation".into();
    crate::locks::write_tolerant(&table).upsert(managed);
    let response = client
        .get(&url)
        .header(HOST, "pool.test")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        count.load(Ordering::Relaxed) <= connected + 1,
        "refused preparation may connect but must never send request bytes"
    );
    // An in-flight request may have copied the previous fence just before
    // reload. It must refuse promptly, not wait for a pool that cannot exist.
    let managed = crate::locks::read_tolerant(&table)
        .lookup("pool.test")
        .unwrap()
        .clone();
    let (obsolete, maintenance) = Upstreams::start(table.clone(), Duration::from_secs(5));
    let result = tokio::time::timeout(Duration::from_millis(200), obsolete.socket(&managed, 0))
        .await
        .unwrap();
    maintenance.abort();
    assert_eq!(
        result.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    drop(client);
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
}
