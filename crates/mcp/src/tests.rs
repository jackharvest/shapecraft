#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use serde_json::{Value, json};
use shapecraft_kernel_occt::OcctKernel;

use crate::Server;

fn call(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    s.handle_line(&line).unwrap()
}

fn tool(s: &mut Server, name: &str, args: Value) -> Value {
    call(s, 9, "tools/call", json!({ "name": name, "arguments": args }))["result"].clone()
}

#[test]
fn an_agent_models_a_bracket_only_through_mcp() {
    let mut s = Server::headless(Arc::new(OcctKernel::new()));
    let init = call(&mut s, 1, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert_eq!(init["result"]["serverInfo"]["name"], "shapecraft");
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    let tools = call(&mut s, 2, "tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "execute"));

    let exec = |s: &mut Server, c: &str, p: Value| tool(s, "execute", json!({ "command": c, "params": p }));
    exec(&mut s, "sketch", json!({ "plane": "xy" }));
    exec(&mut s, "rectangle", json!({ "center": [0, 0], "size": [40, 20] }));
    let r = exec(&mut s, "extrude", json!({ "sketch": 1, "region": 0, "distance": 5 }));
    assert!(r.get("isError").is_none(), "{r}");
    let info: Value = serde_json::from_str(tool(&mut s, "inspect", json!({}))["content"][0]["text"].as_str().unwrap()).unwrap();
    let body = &info["bodies"][0];
    assert!((body["volume_mm3"].as_f64().unwrap() - 4000.0).abs() < 1e-6);
    let top = body["faces"].as_array().unwrap().iter().find(|f| f["normal"] == json!([0.0, 0.0, 1.0])).unwrap()["face"].as_u64().unwrap();
    exec(&mut s, "sketch", json!({ "body": body["id"], "face": top }));
    exec(&mut s, "circle", json!({ "center": [0, 0], "radius": 3 }));
    exec(&mut s, "extrude", json!({ "sketch": 3, "region": 0, "distance": -5 }));
    let info: Value = serde_json::from_str(tool(&mut s, "inspect", json!({}))["content"][0]["text"].as_str().unwrap()).unwrap();
    let v = info["bodies"][0]["volume_mm3"].as_f64().unwrap();
    assert!((v - (4000.0 - std::f64::consts::PI * 9.0 * 5.0)).abs() < 1e-3, "{v}");

    let bad = exec(&mut s, "fillet", json!({ "body": 2, "radius": 500 }));
    assert_eq!(bad["isError"], true);
}

#[test]
fn protocol_errors() {
    let mut s = Server::headless(Arc::new(OcctKernel::new()));
    assert_eq!(call(&mut s, 1, "nope", json!({}))["error"]["code"], -32601);
    assert_eq!(call(&mut s, 2, "tools/call", json!({ "name": "nope" }))["error"]["code"], -32602);
    assert_eq!(call(&mut s, 3, "tools/call", json!({ "name": "execute", "arguments": [] }))["error"]["code"], -32602);
    assert_eq!(s.handle_line("{not json").unwrap()["error"]["code"], -32700);
    let r = tool(&mut s, "execute", json!({}));
    assert_eq!(r["isError"], true);
}
