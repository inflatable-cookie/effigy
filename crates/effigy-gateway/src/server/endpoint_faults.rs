//! Engine fault proof against the actual user-owned daemon, watcher and HTTP.
use super::*;
use crate::endpoint_store::EndpointStore;
use crate::routes::{Route, RouteSource};
use http_body_util::Full;
use hyper::{body::Bytes, service::service_fn, Response};
use hyper_util::rt::TokioIo;

struct Cancel(effigy_kernel::ctx::CancelToken);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.cancel(effigy_kernel::ctx::StopCause::Requested);
    }
}
struct Abort(tokio::task::JoinHandle<()>);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// `publish_current_gateway` looks up the PID parent and does not create it.
/// An operator tempfile without `.effigy/gateway` therefore fails at
/// ClaimPublishBefore with `Io(NotFound)` and never reaches ENOSPC.
#[cfg(unix)]
#[test]
fn publish_fails_with_not_found_when_gateway_parent_is_absent() {
    let home = tempfile::tempdir().unwrap();
    let pid_path = home.path().join(".effigy/gateway/gateway.pid");
    let error =
        crate::identity::publish_current_gateway(&pid_path, nix::unistd::Uid::effective().as_raw())
            .expect_err("absent gateway parent fails before identity write");
    match error {
        GatewayError::Io(io) => {
            assert_eq!(io.kind(), std::io::ErrorKind::NotFound, "{io}");
        }
        other => panic!("expected Io(NotFound), got {other:?}"),
    }
    assert!(!pid_path.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn faults_engine_gateway_endpoint_commit() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = upstream.local_addr().unwrap().to_string();
    let _upstream = Abort(tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            let (stream, _) = upstream.accept().await.unwrap();
            clients.spawn(async move {
                let service = service_fn(|_: hyper::Request<hyper::body::Incoming>| async {
                    Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from_static(
                        b"committed",
                    ))))
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
            while clients.try_join_next().is_some() {}
        }
    }));
    for point in [
        "ClaimPublishBefore",
        "ClaimPublishAfter",
        "ClaimRetainBefore",
        "ClaimRetainAfter",
    ] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".effigy/gateway");
        // Operator publish does not create its parent; setup does.
        crate::private_state::create_owner_only_directory(&root)
            .expect("owner-only operator gateway directory");
        assert!(
            root.is_dir(),
            "operator identity publication requires an owned gateway directory"
        );
        let mut config = GatewayConfig::standard(root.clone());
        // Only fixture-owned, dynamically selected unprivileged endpoints.
        // Keep the TCP selection bound until immediately before daemon start.
        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        config.proxy.bind_addr = http.local_addr().unwrap();
        let address = config.proxy.bind_addr;
        config.proxy.tls_bind_addr = Some("127.0.0.1:0".parse().unwrap());
        let dns = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        config.dns.bind_addr = dns.local_addr().unwrap();
        drop(dns);
        config.tls = None;
        let store = EndpointStore::for_routes(&config.route_table_path);
        store.snapshot().unwrap();
        let cancel = effigy_kernel::ctx::CancelToken::default();
        let _cancel = Cancel(cancel.clone());
        drop(http);
        let daemon_config = config.clone();
        let daemon = tokio::spawn(run_operator_gateway(daemon_config, cancel.clone()));
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let url = format!("http://{address}/");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !config.pid_file_path.exists() {
            if daemon.is_finished() {
                panic!(
                    "daemon failed before identity publication at {point}: {:?}",
                    daemon.await
                );
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "daemon startup timed out"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let before = client
            .get(&url)
            .header("Host", "fault.test")
            .send()
            .await
            .unwrap();
        assert_eq!(before.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
        drop(before);
        let outcome = effigy_kernel::fault_injection::with_hook(
            move |name| {
                if name == point {
                    Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
                } else {
                    Ok(())
                }
            },
            || {
                store.routes::<_, GatewayError>(|table| {
                    table.upsert(Route {
                        domain: "fault.test".into(),
                        target: Some(target.clone()),
                        dns_ip: None,
                        tcp_port: None,
                        tcp_target: None,
                        source: RouteSource::Task,
                        project: home.path().display().to_string(),
                        managed_listener: None,
                        tls: false,
                        registered: chrono::Utc::now(),
                    });
                    Ok(())
                })
            },
        );
        let err = outcome.expect_err(&format!("fault must cross {point}"));
        assert!(
            err.to_string().contains("No space left")
                || matches!(&err, GatewayError::Io(io) if io.raw_os_error() == Some(libc::ENOSPC)),
            "{point} must be ENOSPC, got {err:?}"
        );
        let recovered = store.snapshot().unwrap();
        // Only ClaimPublishAfter sits after the SQLite commit. Retain* fire
        // inside the transaction and roll back.
        let committed = point == "ClaimPublishAfter";
        assert_eq!(
            recovered.routes().unwrap().lookup("fault.test").is_some(),
            committed,
            "{point} outcome={err:?} snapshot={recovered:?}"
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let response = client
                .get(&url)
                .header("Host", "fault.test")
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body = response.text().await.unwrap();
            if committed && status == reqwest::StatusCode::OK {
                assert_eq!(body, "committed");
                break;
            }
            if !committed {
                assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "committed route did not reload: {point}: {body}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(client);
        cancel.cancel(effigy_kernel::ctx::StopCause::Requested);
        tokio::time::timeout(Duration::from_secs(12), daemon)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!config.pid_file_path.exists());
    }
}
