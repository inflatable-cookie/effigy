//! Generates a disposable legacy -> Store -> legacy fixture for the released
//! binary qualification selector. Never points at the operator's HOME.
use effigy_gateway::endpoint_store::EndpointStore;
use serde_json::json;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected fresh fixture HOME")?,
    );
    if !home.is_absolute() || home.exists() {
        return Err("fixture HOME must be a new absolute directory".into());
    }
    std::fs::create_dir_all(home.join(".effigy/gateway"))?;
    let route = |domain: &str| json!({"domain":domain,"target":"127.0.0.1:18080","source":"manual","project":"/fixture/site","tls":false,"registered":"2026-01-01T00:00:00Z","future-route":{"preserved":true}});
    let documents = [
        json!({"_managed_by":"effigy-gateway-route-table-v1","routes":{
            "original.test":route("original.test"),
            "database.test":{"domain":"database.test","dns_ip":"127.1.0.7","tcp_port":15432,"tcp_target":"127.0.0.1:18081","source":"manual","project":"/fixture/site","registered":"2026-01-01T00:00:00Z"}
        }}),
        json!({"assignments":{"site":{"ip":"127.1.0.7","scope":"/fixture/site","future-ip":"preserved"}}}),
        json!({"allocations":{"site":{"base":18000,"range":100,"project":"/fixture/site","assigned_ports":{"web:80":18080},"future-pin":"preserved"}}}),
    ];
    let paths = [
        home.join(".effigy/gateway/routes.json"),
        home.join(".effigy/gateway/loopback-ips.json"),
        home.join(".effigy/ports.json"),
    ];
    for (path, document) in paths.iter().zip(&documents) {
        std::fs::write(path, serde_json::to_vec(document)?)?;
    }
    let store = EndpointStore::new(&home);
    store.import()?;
    store.edit(None, |snapshot| {
        let mut table = snapshot.routes()?;
        let mut route = table
            .lookup("original.test")
            .expect("fixture route")
            .clone();
        route.domain = "added.test".into();
        table.upsert(route);
        snapshot.set_routes(&table)?;
        let mut ips = snapshot.loopback()?;
        ips.assignments.insert(
            "added".into(),
            effigy_gateway::loopback::LoopbackAssignment {
                ip: "127.1.0.8".parse().expect("fixture IP"),
                scope: "/fixture/added".into(),
            },
        );
        snapshot.set_loopback(&ips)?;
        let mut ports = snapshot.ports()?;
        ports
            .allocations
            .get_mut("site")
            .expect("fixture allocation")
            .assigned_ports
            .insert("database:5432".into(), 18081);
        snapshot.set_ports(&ports)
    })?;
    let expected = store.read_compatible()?;
    // No export occurs during ordinary mutations.
    for (path, document) in paths.iter().zip(&documents) {
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path)?)?,
            *document
        );
    }
    store.export_legacy()?;
    for ((family, path), original) in ["routes", "loopback", "ports"]
        .into_iter()
        .zip(&paths)
        .zip(&documents)
    {
        let exported: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        assert_eq!(&exported, expected.document(family)?);
        let entries = match family {
            "routes" => "routes",
            "loopback" => "assignments",
            _ => "allocations",
        };
        for (key, value) in original[entries].as_object().expect("fixture map") {
            if family != "ports" {
                assert_eq!(&exported[entries][key], value);
            } else {
                assert_eq!(
                    exported[entries][key]["assigned_ports"]["web:80"],
                    value["assigned_ports"]["web:80"]
                );
                assert_eq!(exported[entries][key]["future-pin"], value["future-pin"]);
            }
        }
    }
    std::fs::write(
        home.join("expected.json"),
        serde_json::to_vec(&json!({
            "routes":expected.document("routes")?,"loopback":expected.document("loopback")?,"ports":expected.document("ports")?
        }))?,
    )?;
    println!(
        "legacy -> Store -> legacy round trip passed: {}",
        home.display()
    );
    Ok(())
}
