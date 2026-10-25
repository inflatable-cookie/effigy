//! Identical loopback workload for the before/after latency proof.
use super::*;
use crate::identity::read_live_process_identity;
use crate::routes::{ManagedListenerRouteOwner, Route, RouteSource};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

type Sender = hyper::client::conn::http1::SendRequest<Empty<Bytes>>;
async fn client(address: SocketAddr) -> (Sender, tokio::task::JoinHandle<()>) {
    let stream = TcpStream::connect(address).await.unwrap();
    stream.set_nodelay(true).unwrap();
    let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    let driver = tokio::spawn(async move {
        let _ = connection.await;
    });
    (sender, driver)
}
async fn measure(sender: &mut Sender, count: usize) -> (Duration, Duration) {
    let start = Instant::now();
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        sender.ready().await.unwrap();
        let now = Instant::now();
        let request = Request::builder()
            .uri("/module.js")
            .header(HOST, "performance.test")
            .body(Empty::<Bytes>::new())
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        assert_eq!(body, "export default 1;");
        samples.push(now.elapsed());
    }
    let total = start.elapsed();
    samples.sort_unstable();
    (samples[samples.len() / 2], total)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "bounded performance proof; run proof:gateway:proxy:latency"]
async fn loopback_keep_alive_latency_766() {
    let fixture = tempfile::tempdir().unwrap();
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let connections = Arc::new(AtomicUsize::new(0));
    let count = connections.clone();
    let upstream_task = tokio::spawn(async move {
        loop {
            let (stream, _) = upstream.accept().await.unwrap();
            stream.set_nodelay(true).unwrap();
            count.fetch_add(1, Ordering::Relaxed);
            tokio::spawn(async move {
                let service = service_fn(|_: Request<Incoming>| async {
                    Ok::<_, std::convert::Infallible>(Response::new(Full::new(Bytes::from_static(
                        b"export default 1;",
                    ))))
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    let route = Route {
        domain: "performance.test".into(),
        target: Some(upstream_addr.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Task,
        project: fixture.path().display().to_string(),
        scope: None,
        managed_listener: None,
        tls: false,
        registered: chrono::Utc::now(),
    };
    let mut table = RouteTable::new();
    table.upsert(route.clone());
    let table = Arc::new(RwLock::new(table));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let proxy = tokio::spawn(run_proxy_server_on(
        listener,
        ProxyConfig::default(),
        table.clone(),
        Arc::new(GatewayStats::new()),
        shutdown,
    ));
    let (mut direct, direct_driver) = client(upstream_addr).await;
    measure(&mut direct, 8).await;
    let (direct_p50, direct_total) = measure(&mut direct, 766).await;
    let (mut routed, routed_driver) = client(address).await;
    measure(&mut routed, 8).await;
    let plain_connections = connections.load(Ordering::Relaxed);
    let (plain_p50, plain_total) = measure(&mut routed, 766).await;
    let plain_opened = connections.load(Ordering::Relaxed) - plain_connections;

    let identity = read_live_process_identity(std::process::id()).unwrap();
    let availability = fixture.path().join("listener.json");
    std::fs::write(&availability, serde_json::to_vec(&serde_json::json!({"schema":"effigy.managed.host-listener-state.v1", "owner":"performance", "runtime_generation":"runtime", "generation":"child", "status":"ready"})).unwrap()).unwrap();
    let owner = ManagedListenerRouteOwner {
        run: None,
        owner: "performance".into(),
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
        address: upstream_addr.to_string(),
        route_tls: false,
        availability_file: availability.display().to_string(),
    };
    crate::managed_listener::verify_managed_listener_owner(&owner).unwrap();
    crate::locks::write_tolerant(&table).upsert(Route {
        managed_listener: Some(owner),
        ..route
    });
    measure(&mut routed, 8).await;
    let proofs_before = crate::managed_listener::OWNERSHIP_PROOFS.load(Ordering::Relaxed);
    let managed_connections = connections.load(Ordering::Relaxed);
    let (managed_p50, managed_total) = measure(&mut routed, 766).await;
    let managed_opened = connections.load(Ordering::Relaxed) - managed_connections;
    let managed_proofs =
        crate::managed_listener::OWNERSHIP_PROOFS.load(Ordering::Relaxed) - proofs_before;
    eprintln!("managed background ownership proofs during 766 requests: {managed_proofs}");
    eprintln!("gateway-latency requests=766 direct_p50_ms={:.3} plain_p50_ms={:.3} managed_p50_ms={:.3} direct_total_s={:.3} plain_total_s={:.3} managed_total_s={:.3} plain_upstream_connections={} managed_upstream_connections={}",
        direct_p50.as_secs_f64()*1000., plain_p50.as_secs_f64()*1000., managed_p50.as_secs_f64()*1000.,
        direct_total.as_secs_f64(), plain_total.as_secs_f64(), managed_total.as_secs_f64(), plain_opened, managed_opened);
    if std::env::var("EFFIGY_GATEWAY_PERF_BASELINE").as_deref() != Ok("1") {
        assert!(
            managed_proofs < 32,
            "ownership proof ran on the request path"
        );
        assert!(plain_p50.saturating_sub(direct_p50) < Duration::from_millis(5));
        assert!(managed_p50.saturating_sub(direct_p50) < Duration::from_millis(5));
        assert!(
            plain_opened < 8 && managed_opened < 8,
            "upstream connections were not reused"
        );
    }
    drop(direct);
    drop(routed);
    direct_driver.await.unwrap();
    routed_driver.await.unwrap();
    stop.send(true).unwrap();
    proxy.await.unwrap().unwrap();
    upstream_task.abort();
    let _ = upstream_task.await;
}
