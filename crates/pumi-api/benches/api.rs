use divan::black_box;
use pumi_api::{Caller, Registry, rpc};
use serde_json::json;
fn main() {
    divan::main();
}
#[divan::bench]
fn dispatch_call_batch(bencher: divan::Bencher) {
    let registry = Registry::new();
    let caller = Caller::default();
    bencher.bench_local(|| registry.call(&mut (), &caller, "call", json!([[]])).unwrap());
}
#[divan::bench]
fn rpc_round_trip(bencher: divan::Bencher) {
    let registry = Registry::new();
    let caller = Caller::default();
    let text = r#"{"jsonrpc":"2.0","id":1,"method":"call","params":[[{"name":"call","args":[[]]}]]}"#;
    bencher.bench_local(|| rpc::handle(&registry, &mut (), &caller, black_box(text)));
}
