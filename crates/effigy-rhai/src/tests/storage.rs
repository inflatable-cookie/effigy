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
