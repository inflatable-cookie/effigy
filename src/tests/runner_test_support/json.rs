pub(crate) fn parse_json_output(rendered: &str) -> serde_json::Value {
    serde_json::from_str(rendered).expect("parse json")
}
