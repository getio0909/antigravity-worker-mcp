use serde_json::json;
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--sentinel") {
        for count in 0..600 {
            fs::write(&args[1], count.to_string()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return;
    }
    if args.contains(&"--version".into()) {
        println!("1.2.14-fixture");
        return;
    }
    if args.first().is_some_and(|a| a == "models") {
        println!("fixture-fast\tFixture Fast\nfixture-deep\tFixture Deep");
        return;
    }
    let mut body = String::new();
    io::stdin().read_to_string(&mut body).unwrap();
    let input: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
    let text = input["message"]["content"].as_str().unwrap();
    let model = args
        .iter()
        .position(|a| a == "--model")
        .map(|i| args[i + 1].clone())
        .unwrap_or_default();
    println!("{}", json!({"event":"init","init":{"model":model}}));
    if text.contains("CASE:malformed") {
        println!("invalid-json");
        return;
    }
    if text.contains("CASE:output-limit") {
        println!("{}", "x".repeat(2200000));
        return;
    }
    if text.contains("CASE:wait") {
        std::thread::sleep(std::time::Duration::from_secs(60));
        return;
    }
    if text.contains("CASE:quota") {
        println!(
            "{}",
            json!({"event":"result","result":{"status":"ERROR","error":"resource exhausted","response":""}})
        );
        return;
    }
    let work = if std::path::Path::new("/work").exists() {
        PathBuf::from("/work")
    } else {
        std::env::current_dir().unwrap()
    };
    if text.contains("CASE:child") {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--sentinel")
            .arg(work.join("child-alive.txt"))
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(60));
        child.wait().unwrap();
        return;
    }
    if text.contains("CASE:modify") {
        fs::write(work.join("example.txt"), "updated\n").unwrap();
        fs::write(work.join("new.txt"), "added\n").unwrap();
    }
    let mut denied: Vec<&str> = vec![];
    if text.contains("CASE:readonly") && fs::write(work.join("example.txt"), "changed\n").is_err() {
        denied.push("write_file");
    }
    if text.contains("CASE:full-permissions") {
        assert!(args.contains(&"--dangerously-skip-permissions".into()));
        assert!(!std::path::Path::new("/work").exists());
        fs::write(work.join("direct.txt"), "host-execution-enabled\n").unwrap();
    }
    let report = json!({"summary":"Fixture report \u{e9}\u{1f680}","findings":[],"limitations":[]});
    println!(
        "{}",
        json!({"event":"step_update","step_update":{"state":"DONE","step_type":"agent_response","text_delta":"ok"}})
    );
    println!(
        "{}",
        json!({"event":"result","result":{"status":"SUCCESS","response":report.to_string(),"structured_output":report,"denied_actions":denied,"usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7}}})
    );
}
