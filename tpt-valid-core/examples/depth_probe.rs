//! Standalone probe for the deep-document depth guard (debugging aid).

use tpt_valid_core::{validate, ValidationNode, ValidationOptions};

fn main() {
    // Large stack: dropping the pathological value itself recurses to its
    // full depth; production callers are protected by the engine guard and
    // parser recursion limits instead.
    std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(real_main)
        .unwrap()
        .join()
        .unwrap();
}

fn real_main() {
    let depth: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    println!("building node ({depth})...");
    let mut node = ValidationNode::Always;
    for _ in 0..depth {
        node = ValidationNode::CheckField("child".into(), Box::new(node));
    }
    println!("building value...");
    let mut value = serde_json::Value::Number(1.into());
    for _ in 0..depth {
        let mut map = serde_json::Map::new();
        map.insert("child".to_string(), value);
        value = serde_json::Value::Object(map);
    }
    println!("validating...");
    let errs = validate(&node, &value, &ValidationOptions::default());
    println!(
        "errors: {} (first path len {})",
        errs.len(),
        errs[0].path.len()
    );
    println!("done");
}
