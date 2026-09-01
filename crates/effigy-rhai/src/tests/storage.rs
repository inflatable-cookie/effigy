use super::*;

#[test]
fn execute_rhai_script_exposes_storage_provider_and_status() {
    let root = temp_root("storage-provider");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "storage".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        if storage::provider() != "s3" { throw("provider"); }
        if storage::provider(#{ provider: "s3" }) != "s3" { throw("provider explicit"); }
        let status = storage::status(#{
            bucket: "assets",
            region: "eu-west-2",
            endpoint: "http://127.0.0.1:9000",
            path_style: true,
            access_key_id: "test-access",
            secret_access_key: "test-secret",
        });
        if status["provider"] != "s3" { throw("status provider"); }
        if status["adapter"] != "s3" { throw("status adapter"); }
        if status["bucket"] != "assets" { throw("status bucket"); }
        if status["region"] != "eu-west-2" { throw("status region"); }
        if status["path_style"] != true { throw("status path style"); }
        if status["explicit_credentials"] != true { throw("status credentials"); }
        if status["ready"] != true { throw("status ready"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_routes_storage_operations_through_s3_adapter() {
    let root = temp_root("storage-s3");
    let download_path = root.join("downloads/object.txt");
    let upload_path = root.join("upload.txt");
    fs::write(&upload_path, "upload body").expect("upload body");
    let request_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 8192];
            let bytes_read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
            let request_line = request.lines().next().unwrap_or_default().to_owned();
            server_log.lock().expect("log").push(request_line.clone());

            if request_line.starts_with("GET /assets?list-type=2") {
                let body = concat!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
                    "<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">",
                    "<Name>assets</Name><Prefix></Prefix><KeyCount>1</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated>",
                    "<Contents><Key>docs/readme.txt</Key><ETag>&quot;etag-list&quot;</ETag><Size>11</Size></Contents>",
                    "<CommonPrefixes><Prefix>docs/</Prefix></CommonPrefixes>",
                    "</ListBucketResult>"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).expect("write list");
            } else if request_line.starts_with("HEAD /assets/docs/readme.txt ")
                || request_line.starts_with("HEAD /assets/docs/readme.txt?")
            {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nETag: \"etag-head\"\r\nContent-Type: text/plain\r\nContent-Length: 11\r\nx-amz-meta-source: fixture\r\n\r\n",
                    )
                    .expect("write head");
            } else if request_line.starts_with("GET /assets/docs/readme.txt ") {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nETag: \"etag-get\"\r\nContent-Type: text/plain\r\nContent-Length: 11\r\nx-amz-meta-source: fixture\r\n\r\nhello world",
                    )
                    .expect("write get");
            } else if request_line.starts_with("PUT /assets/uploads/body.txt ") {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nETag: \"etag-put\"\r\nContent-Length: 0\r\n\r\n",
                    )
                    .expect("write put");
            } else if request_line.starts_with("DELETE /assets/uploads/body.txt ") {
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .expect("write delete");
            } else {
                panic!("unexpected request: {request_line}\n{request}");
            }
        }
    });

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root.clone(),
        task_name: "storage".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = format!(
        r#"
        let base = #{{
            bucket: "assets",
            region: "us-east-1",
            endpoint: "http://{}",
            path_style: true,
            access_key_id: "test-access",
            secret_access_key: "test-secret",
        }};

        let listed = storage::ls(base);
        if listed["objects"].len() != 1 {{ throw("list objects"); }}
        if listed["objects"][0]["key"] != "docs/readme.txt" {{ throw("list key"); }}
        if listed["common_prefixes"][0] != "docs/" {{ throw("list prefix"); }}

        let head = storage::head(base + #{{ key: "docs/readme.txt" }});
        if head["e_tag"] != "\"etag-head\"" {{ throw("head etag"); }}
        if head["content_length"] != 11 {{ throw("head content length"); }}
        if head["metadata"]["source"] != "fixture" {{ throw("head metadata"); }}

        let fetched = storage::get(base + #{{ key: "docs/readme.txt", path: "downloads/object.txt" }});
        if fetched["path"] != "{}" {{ throw("get path"); }}
        if fetched["size"] != 11 {{ throw("get size"); }}

        let uploaded = storage::put(base + #{{ key: "uploads/body.txt", path: "upload.txt", content_type: "text/plain" }});
        if uploaded["e_tag"] != "\"etag-put\"" {{ throw("put etag"); }}
        if uploaded["size"] != 11 {{ throw("put size"); }}

        let deleted = storage::delete(base + #{{ key: "uploads/body.txt" }});
        if !deleted["success"] {{ throw("delete success"); }}
    "#,
        address,
        download_path.display()
    );

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server join");

    assert_eq!(
        fs::read_to_string(&download_path).expect("downloaded file"),
        "hello world"
    );
    let log = request_log.lock().expect("request log");
    assert!(
        log.iter()
            .any(|line| line.starts_with("GET /assets?list-type=2")),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|line| line.starts_with("HEAD /assets/docs/readme.txt")),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|line| line.starts_with("PUT /assets/uploads/body.txt")),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|line| line.starts_with("DELETE /assets/uploads/body.txt")),
        "{log:?}"
    );
}

#[test]
fn rhai_surface_registry_lists_storage_module() {
    let surface = crate::surface::rhai_surface_json();
    assert!(surface["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .any(|module| module.as_str() == Some("storage")));
    assert!(surface["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .any(|function| function["module"] == "storage" && function["name"] == "put"));
}
struct FixtureRequest {
    request_line: String,
    head: String,
    body: Vec<u8>,
}

fn read_fixture_request(stream: &mut std::net::TcpStream) -> FixtureRequest {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).expect("read request head");
        if read == 0 {
            panic!("stream closed before request head");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_owned();
    let mut content_length = 0_usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().expect("parse content length");
        }
    }
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).expect("read request body");
        if read == 0 {
            panic!("stream closed before request body");
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    FixtureRequest {
        request_line,
        head,
        body,
    }
}

fn storage_script_context(root: &std::path::Path) -> ScriptContext {
    ScriptContext {
        cwd: root.to_path_buf(),
        repo_root: root.to_path_buf(),
        task_name: "storage".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    }
}

fn storage_base_options(address: std::net::SocketAddr) -> String {
    format!(
        "#{{ bucket: \"assets\", region: \"us-east-1\", endpoint: \"http://{address}\", \
         path_style: true, access_key_id: \"test-access\", secret_access_key: \"test-secret\" }}"
    )
}

#[test]
fn execute_rhai_script_unconditional_head_then_put_lets_second_writer_replace_winner() {
    let root = temp_root("storage-head-then-put-race");
    let stored = Arc::new(Mutex::new(Vec::<u8>::new()));
    let request_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_stored = Arc::clone(&stored);
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        // Deterministic interleaving that models the race: both writers
        // observe absence before either unconditional PUT lands, and the
        // server accepts both writes last-value-wins.
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_fixture_request(&mut stream);
            let request_line = request.request_line.clone();
            server_log.lock().expect("log").push(request_line.clone());
            if request_line.starts_with("HEAD /assets/media/clip.mp4") {
                stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .expect("write 404");
            } else if request_line.starts_with("PUT /assets/media/clip.mp4 ") {
                *server_stored.lock().expect("store") = request.body;
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nETag: \"etag-put\"\r\nContent-Length: 0\r\n\r\n",
                    )
                    .expect("write 200");
            } else {
                panic!("unexpected request: {request_line}");
            }
        }
    });

    let context = storage_script_context(&root);
    let script = format!(
        r#"
        let base = {base};

        let winner_occupied = true;
        try {{ storage::head(base + #{{ key: "media/clip.mp4" }}); }} catch (err) {{ winner_occupied = false; }}
        let loser_occupied = true;
        try {{ storage::head(base + #{{ key: "media/clip.mp4" }}); }} catch (err) {{ loser_occupied = false; }}

        if winner_occupied {{ throw("winner saw occupation"); }}
        if loser_occupied {{ throw("loser saw occupation"); }}

        let winner = storage::put(base + #{{ key: "media/clip.mp4", body: "winner-bytes", content_type: "video/mp4", metadata: #{{ writer: "one" }} }});
        if !winner["success"] {{ throw("winner put failed"); }}
        let loser = storage::put(base + #{{ key: "media/clip.mp4", body: "loser-bytes", content_type: "video/mp4", metadata: #{{ writer: "two" }} }});
        if !loser["success"] {{ throw("loser put failed"); }}
    "#,
        base = storage_base_options(address)
    );

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server join");

    assert_eq!(
        *stored.lock().expect("store"),
        b"loser-bytes".to_vec(),
        "second writer replaced the first winner's bytes"
    );
    let log = request_log.lock().expect("request log");
    assert_eq!(
        log.iter().filter(|line| line.starts_with("HEAD ")).count(),
        2,
        "both writers performed a preliminary HEAD: {log:?}"
    );
    assert_eq!(
        log.iter().filter(|line| line.starts_with("PUT ")).count(),
        2,
        "both writers performed an unconditional PUT: {log:?}"
    );
    assert!(
        log.iter()
            .all(|line| !line.to_ascii_lowercase().contains("if-none-match")),
        "unconditional puts carry no precondition: {log:?}"
    );
}
fn fixture_header_value(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (header, value) = line.split_once(':')?;
        if header.trim().eq_ignore_ascii_case(name) {
            Some(value.trim().to_owned())
        } else {
            None
        }
    })
}

fn fixture_put_accepted(etag: &str) -> String {
    format!("HTTP/1.1 200 OK\r\nETag: {etag}\r\nContent-Length: 0\r\n\r\n")
}

/// Collision response carrying a signed URL, credential-shaped strings, and a
/// hostile request id in the body and headers. `status` selects the provider
/// spelling: `412 Precondition Failed` with code `PreconditionFailed`, or the
/// S3 PutObject-documented `409 Conflict` with code
/// `ConditionalRequestConflict`.
fn fixture_hostile_collision_response(status: &str, code: &str) -> String {
    let body = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Error><Code>{}</Code>",
            "<Message>precondition failed; see https://assets.s3.amazonaws.com/media/clip.mp4",
            "?X-Amz-Signature=deadbeefsignedpayload&amp;X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20260901",
            "%2Fus-east-1%2Fs3%2Faws4_request wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY</Message>",
            "<RequestId>HOSTILE-REQUEST-ID</RequestId><HostId>HOSTILE-HOST-ID</HostId></Error>"
        ),
        code
    );
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/xml\r\n\
         x-amz-request-id: HOSTILE-REQUEST-ID\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

fn fixture_precondition_failed_response() -> String {
    fixture_hostile_collision_response("412 Precondition Failed", "PreconditionFailed")
}

fn fixture_condition_conflict_response() -> String {
    fixture_hostile_collision_response("409 Conflict", "ConditionalRequestConflict")
}

const STABLE_COLLISION_MESSAGE: &str =
    "storage::put create_only failed: key \"media/clip.mp4\" already exists in bucket \"assets\"";

fn assert_no_hostile_material(message: &str) {
    for hostile in [
        "X-Amz-Signature",
        "AKIAIOSFODNN7EXAMPLE",
        "wJalrXUtnFEMI",
        "deadbeefsignedpayload",
        "HOSTILE-REQUEST-ID",
        "HOSTILE-HOST-ID",
        "PreconditionFailed",
        "ConditionalRequestConflict",
        "precondition failed; see",
    ] {
        assert!(
            !message.contains(hostile),
            "diagnostic leaked hostile material `{hostile}`: {message}"
        );
    }
}

type StoredObject = Arc<Mutex<Option<(Vec<u8>, String)>>>;

#[test]
fn execute_rhai_script_create_only_yields_exactly_one_winner_and_redacts_collision() {
    let root = temp_root("storage-create-only-winner");
    let stored: StoredObject = Arc::new(Mutex::new(None));
    let request_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_stored = Arc::clone(&stored);
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        // Two create-only writers race for one absent key: the fixture
        // atomically accepts the first conditional PUT and refuses the
        // second with a hostile precondition response.
        let mut occupied = false;
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_fixture_request(&mut stream);
            let request_line = request.request_line.clone();
            server_log.lock().expect("log").push(request_line.clone());
            if !request_line.starts_with("PUT /assets/media/clip.mp4 ") {
                panic!("unexpected request: {request_line}");
            }
            if !occupied {
                *server_stored.lock().expect("store") = Some((
                    request.body,
                    fixture_header_value(&request.head, "x-amz-meta-writer").unwrap_or_default(),
                ));
                occupied = true;
                stream
                    .write_all(fixture_put_accepted("\"etag-winner\"").as_bytes())
                    .expect("write 200");
            } else {
                stream
                    .write_all(fixture_precondition_failed_response().as_bytes())
                    .expect("write 412");
            }
        }
    });

    let context = storage_script_context(&root);
    let base = storage_base_options(address);
    let winner_script = format!(
        r#"
        let base = {base};

        let uploaded = storage::put(base + #{{ key: "media/clip.mp4", body: "winner-bytes", content_type: "video/mp4", metadata: #{{ writer: "one" }}, create_only: true }});
        if !uploaded["success"] {{ throw("winner put failed"); }}
        if uploaded["e_tag"] != "\"etag-winner\"" {{ throw("winner etag"); }}
    "#,
        base = base
    );
    execute_rhai_script(&context, &winner_script, &[], &callbacks()).expect("winner execute");
    let loser_script = format!(
        r#"
        let base = {base};

        storage::put(base + #{{ key: "media/clip.mp4", body: "loser-bytes", content_type: "video/mp4", metadata: #{{ writer: "two" }}, create_only: true }});
    "#,
        base = base
    );
    let error = execute_rhai_script(&context, &loser_script, &[], &callbacks())
        .expect_err("loser create-only must fail");
    let message = error.to_string();
    assert!(
        message.contains(STABLE_COLLISION_MESSAGE),
        "collision diagnostic was not the stable message: {message}"
    );
    assert_no_hostile_material(&message);

    server.join().expect("server join");
    assert_eq!(
        request_log.lock().expect("log").len(),
        2,
        "each writer sent exactly one PUT with no retry or fallback: {request_log:?}"
    );
    let stored = stored.lock().expect("store");
    let (body, writer) = stored.as_ref().expect("stored winner");
    assert_eq!(body.as_slice(), b"winner-bytes", "winner bytes remain");
    assert_eq!(writer, "one", "winner metadata remains");
}

#[test]
fn execute_rhai_script_create_only_over_occupied_key_refuses_without_mutation() {
    let root = temp_root("storage-create-only-occupied");
    let stored: StoredObject = Arc::new(Mutex::new(None));
    let request_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_stored = Arc::clone(&stored);
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        // Seed the key with an ordinary PUT, then refuse a create-only PUT
        // carrying different bytes and metadata.
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_fixture_request(&mut stream);
            let request_line = request.request_line.clone();
            server_log.lock().expect("log").push(request_line.clone());
            if !request_line.starts_with("PUT /assets/media/clip.mp4 ") {
                panic!("unexpected request: {request_line}");
            }
            let incoming_writer =
                fixture_header_value(&request.head, "x-amz-meta-writer").unwrap_or_default();
            let was_occupied = server_stored.lock().expect("store").is_some();
            if !was_occupied {
                *server_stored.lock().expect("store") = Some((request.body, incoming_writer));
                stream
                    .write_all(fixture_put_accepted("\"etag-seed\"").as_bytes())
                    .expect("write 200");
            } else {
                stream
                    .write_all(fixture_precondition_failed_response().as_bytes())
                    .expect("write 412");
            }
        }
    });

    let context = storage_script_context(&root);
    let base = storage_base_options(address);
    let seed_script = format!(
        r#"
        let base = {base};

        storage::put(base + #{{ key: "media/clip.mp4", body: "seeded-bytes", content_type: "video/mp4", metadata: #{{ writer: "seed" }} }});
    "#,
        base = base
    );
    execute_rhai_script(&context, &seed_script, &[], &callbacks()).expect("seed execute");

    let attacker_script = format!(
        r#"
        let base = {base};

        storage::put(base + #{{ key: "media/clip.mp4", body: "attacker-bytes", content_type: "video/mp4", metadata: #{{ writer: "attacker" }}, create_only: true }});
    "#,
        base = base
    );
    let error = execute_rhai_script(&context, &attacker_script, &[], &callbacks())
        .expect_err("create-only over an occupied key must fail");
    let message = error.to_string();
    assert!(
        message.contains(STABLE_COLLISION_MESSAGE),
        "collision diagnostic was not the stable message: {message}"
    );
    assert_no_hostile_material(&message);

    server.join().expect("server join");
    assert_eq!(
        request_log.lock().expect("log").len(),
        2,
        "no retry or unconditional fallback PUT was sent: {request_log:?}"
    );
    let stored = stored.lock().expect("store");
    let (body, writer) = stored.as_ref().expect("seeded object");
    assert_eq!(body.as_slice(), b"seeded-bytes", "seeded bytes remain");
    assert_eq!(writer, "seed", "seeded metadata remains");
}

#[test]
fn execute_rhai_script_create_only_treats_409_conflict_as_redacted_collision() {
    let root = temp_root("storage-create-only-409");
    let stored: StoredObject = Arc::new(Mutex::new(None));
    let request_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_stored = Arc::clone(&stored);
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        // S3 PutObject also reports a conflicting operation during an
        // If-None-Match upload as HTTP 409 ConditionalRequestConflict. The
        // fixture occupies the key, then refuses the create-only writer with
        // a hostile 409: one request, no retry, no fallback, no mutation.
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_fixture_request(&mut stream);
            let request_line = request.request_line.clone();
            server_log.lock().expect("log").push(request_line.clone());
            if !request_line.starts_with("PUT /assets/media/clip.mp4 ") {
                panic!("unexpected request: {request_line}");
            }
            let was_occupied = server_stored.lock().expect("store").is_some();
            if !was_occupied {
                *server_stored.lock().expect("store") = Some((
                    request.body,
                    fixture_header_value(&request.head, "x-amz-meta-writer")
                        .unwrap_or_default(),
                ));
                stream
                    .write_all(fixture_put_accepted("\"etag-winner\"").as_bytes())
                    .expect("write 200");
            } else {
                stream
                    .write_all(fixture_condition_conflict_response().as_bytes())
                    .expect("write 409");
            }
        }
    });

    let context = storage_script_context(&root);
    let base = storage_base_options(address);
    let winner_script = format!(
        r#"
        let base = {base};

        let uploaded = storage::put(base + #{{ key: "media/clip.mp4", body: "winner-bytes", metadata: #{{ writer: "one" }}, create_only: true }});
        if !uploaded["success"] {{ throw("winner put failed"); }}
    "#,
        base = base
    );
    execute_rhai_script(&context, &winner_script, &[], &callbacks()).expect("winner execute");

    let loser_script = format!(
        r#"
        let base = {base};

        storage::put(base + #{{ key: "media/clip.mp4", body: "loser-bytes", metadata: #{{ writer: "two" }}, create_only: true }});
    "#,
        base = base
    );
    let error = execute_rhai_script(&context, &loser_script, &[], &callbacks())
        .expect_err("409 create-only loser must fail");
    let message = error.to_string();
    assert!(
        message.contains(STABLE_COLLISION_MESSAGE),
        "409 collision diagnostic was not the stable message: {message}"
    );
    assert_no_hostile_material(&message);

    server.join().expect("server join");
    assert_eq!(
        request_log.lock().expect("log").len(),
        2,
        "a hostile 409 must not be retried or fall back unconditionally: {request_log:?}"
    );
    let stored = stored.lock().expect("store");
    let (body, writer) = stored.as_ref().expect("stored winner");
    assert_eq!(body.as_slice(), b"winner-bytes", "winner bytes remain");
    assert_eq!(writer, "one", "winner metadata remains");
}

#[test]
fn execute_rhai_script_ordinary_put_over_occupied_key_still_replaces() {
    let root = temp_root("storage-ordinary-put-replace");
    let stored = Arc::new(Mutex::new(Vec::<u8>::new()));
    let request_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_stored = Arc::clone(&stored);
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_fixture_request(&mut stream);
            let request_line = request.request_line.clone();
            server_log.lock().expect("log").push(request_line.clone());
            if !request_line.starts_with("PUT /assets/media/clip.mp4 ") {
                panic!("unexpected request: {request_line}");
            }
            *server_stored.lock().expect("store") = request.body;
            let etag = if server_log.lock().expect("log").len() == 1 {
                "\"etag-seed\""
            } else {
                "\"etag-replace\""
            };
            stream
                .write_all(fixture_put_accepted(etag).as_bytes())
                .expect("write 200");
        }
    });

    let context = storage_script_context(&root);
    let script = format!(
        r#"
        let base = {base};

        storage::put(base + #{{ key: "media/clip.mp4", body: "seeded-bytes" }});
        let replaced = storage::put(base + #{{ key: "media/clip.mp4", body: "replacement-bytes", create_only: false }});
        if !replaced["success"] {{ throw("replace success"); }}
        if replaced["e_tag"] != "\"etag-replace\"" {{ throw("replace etag"); }}
        if replaced["provider"] != "s3" {{ throw("replace provider"); }}
        if replaced["bucket"] != "assets" {{ throw("replace bucket"); }}
        if replaced["key"] != "media/clip.mp4" {{ throw("replace key"); }}
        if replaced["size"] != 17 {{ throw("replace size"); }}
    "#,
        base = storage_base_options(address)
    );
    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server join");

    assert_eq!(
        *stored.lock().expect("store"),
        b"replacement-bytes".to_vec(),
        "ordinary and create_only=false puts keep the existing replacement behavior"
    );
    let log = request_log.lock().expect("request log");
    assert!(
        log.iter()
            .all(|line| !line.to_ascii_lowercase().contains("if-none-match")),
        "omitted and false create_only send no precondition: {log:?}"
    );
}

#[test]
fn execute_rhai_script_create_only_sends_condition_on_the_put_itself() {
    let root = temp_root("storage-create-only-request");
    let request_log = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("local addr");
    let server_log = Arc::clone(&request_log);
    let server = thread::spawn(move || {
        for _ in 0..1 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_fixture_request(&mut stream);
            let request_line = request.request_line.clone();
            let head = request.head.clone();
            server_log.lock().expect("log").push((request_line, head));
            stream
                .write_all(fixture_put_accepted("\"etag-create\"").as_bytes())
                .expect("write 200");
        }
    });

    let context = storage_script_context(&root);
    let script = format!(
        r#"
        let base = {base};

        let uploaded = storage::put(base + #{{ key: "media/clip.mp4", body: "create-bytes", create_only: true }});
        if !uploaded["success"] {{ throw("create-only success"); }}
    "#,
        base = storage_base_options(address)
    );
    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server join");

    let log = request_log.lock().expect("request log");
    assert_eq!(
        log.len(),
        1,
        "create-only must not perform a preliminary HEAD: {log:?}"
    );
    let (request_line, head) = &log[0];
    assert!(
        request_line.starts_with("PUT /assets/media/clip.mp4 "),
        "condition must ride the PUT request: {request_line}"
    );
    assert!(
        head.to_ascii_lowercase().contains("if-none-match: *"),
        "PUT must carry If-None-Match wildcard: {head}"
    );
}

#[test]
fn execute_rhai_script_rejects_non_bool_create_only() {
    let root = temp_root("storage-create-only-type");
    let context = storage_script_context(&root);
    let script = format!(
        r#"
        let base = {base};

        storage::put(base + #{{ key: "media/clip.mp4", body: "payload", create_only: "yes" }});
    "#,
        base = storage_base_options(std::net::SocketAddr::from(([127, 0, 0, 1], 9)))
    );
    let error = execute_rhai_script(&context, &script, &[], &callbacks())
        .expect_err("non-bool create_only must fail parsing");
    assert!(
        error.to_string().contains("`create_only` must be a bool"),
        "got: {error}"
    );
}
