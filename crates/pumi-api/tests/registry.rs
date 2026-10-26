use pumi_api::{Caller, Effect, ErrorKind, FunctionInfo, Registry, RunsOn};
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;
#[test]
fn every_core_function_is_described_and_its_examples_hold() {
    let registry = Registry::new();
    let functions = Registry::core_functions();
    assert!(functions.len() >= 2, "the builtins are registered");
    for function in functions {
        let info = function.info();
        info.validate().unwrap();
        function.check_examples().unwrap();
        assert!(
            info.name.split('.').all(
                |s| s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
            ),
            "{}: core functions are named `noun.verb`, camelCase after the dot",
            info.name
        );
        let verb = info.name.rsplit('.').next().unwrap_or_default();
        if ["get", "list", "eval", "parse"].iter().any(|prefix| verb.starts_with(prefix)) {
            assert_eq!(info.effect, Effect::Read, "{}: get/list/eval/parse functions only read", info.name);
        }
        assert!(
            info.params_schema["additionalProperties"] == json!(false),
            "{}: unknown arguments are errors",
            info.name
        );
        for example in &info.examples {
            let got = registry.call(&mut (), &Caller::default(), &info.name, example.args.clone());
            if let Some(expected) = &example.result {
                assert_eq!(&got.unwrap(), expected, "{}: example {:?}", info.name, example.args);
            } else {
                got.unwrap();
            }
        }
    }
}
#[test]
fn api_info_lists_functions_by_name() {
    let registry = Registry::new();
    let info = registry.call(&mut (), &Caller::default(), "api.getInfo", json!([])).unwrap();
    let names: Vec<&str> = info["functions"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(!names.contains(&"api.getInfo"), "internal: called, not listed");
    assert_eq!(info["error_types"].as_object().unwrap().len(), 7);
    assert_eq!(info["error_types"]["ambiguous"]["exit_code"], 3);
    let call = info["functions"].as_array().unwrap().iter().find(|f| f["name"] == "call").unwrap();
    assert_eq!(call["parameters"], json!([["Object", "name"], ["Object", "args"]]));
    let opened = info["events"].as_array().unwrap().iter().find(|e| e["name"] == "tab.opened").unwrap();
    assert_eq!(opened["provider"], "core");
}
fn info(name: &str, effect: Effect, params: &[&str]) -> FunctionInfo {
    FunctionInfo {
        name: name.into(),
        since: 1,
        provider: "test".into(),
        runs_on: RunsOn::Server,
        effect,
        platforms: Vec::new(),
        desc: format!("{name}, for a test."),
        parameters: params.iter().map(|p| ("Object".to_string(), p.to_string())).collect(),
        params_schema: json!({"type": "object", "additionalProperties": false}),
        returns: "Object".into(),
        returns_schema: json!(true),
        examples: vec![pumi_api::ExampleInfo { args: json!([]), result: None, desc: None }],
        notes: Vec::new(),
        source: "test".into(),
        internal: false,
    }
}
#[test]
fn created_functions_are_called_and_mutations_are_logged_once() {
    let registry = Registry::new();
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    registry
        .create_function(
            info("test.inner", Effect::Write, &["n"]),
            Box::new(move |_, args| {
                counter.set(counter.get() + 1);
                Ok(args)
            }),
        )
        .unwrap();
    registry
        .create_function(
            info("test.outer", Effect::Write, &[]),
            Box::new(|cx, _| cx.call("test.inner", json!({"n": 1}))),
        )
        .unwrap();
    registry.create_function(info("test.read", Effect::Read, &[]), Box::new(|_, _| Ok(json!("r")))).unwrap();
    let caller = Caller { channel: 7, ..Caller::default() };
    let result = registry
        .call(
            &mut (),
            &caller,
            "call",
            json!([[
                { "name": "test.outer" },
                { "name": "test.read", "args": [] },
                { "name": "call", "args": ["test.inner", [2]] }
            ]]),
        )
        .unwrap();
    assert_eq!(result, json!([{"n": 1}, "r", [2]]));
    assert_eq!(calls.get(), 2);
}
#[test]
fn a_batch_stops_at_its_first_error_and_names_the_call() {
    let registry = Registry::new();
    let batch = json!([[{ "name": "call", "args": [[]] }, { "name": "no.such" }]]);
    let err = registry.call(&mut (), &Caller::default(), "call", batch).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(err.message.starts_with("call 1 (no.such): "), "{}", err.message);
}
#[test]
fn created_functions_are_checked() {
    let registry = Registry::new();
    let noop = || -> pumi_api::DynHandler { Box::new(|_, _| Ok(Value::Null)) };
    let err = registry.create_function(info("call", Effect::Read, &[]), noop()).unwrap_err();
    assert!(err.message.contains("core function"));
    let mut undocumented = info("test.undocumented", Effect::Read, &[]);
    undocumented.desc.clear();
    assert!(registry.create_function(undocumented, noop()).is_err());
    let mut no_example = info("test.no_example", Effect::Read, &[]);
    no_example.examples.clear();
    assert!(registry.create_function(no_example, noop()).is_err());
    registry.create_function(info("test.f", Effect::Read, &["a"]), noop()).unwrap();
    assert!(registry.create_function(info("test.f", Effect::Read, &[]), noop()).is_err());
    let caller = Caller::default();
    let err = registry.call(&mut (), &caller, "test.f", json!({"b": 1})).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    let err = registry.call(&mut (), &caller, "test.f", json!([1, 2])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    registry.del_function("test.f").unwrap();
    assert_eq!(registry.call(&mut (), &caller, "test.f", json!([])).unwrap_err().kind, ErrorKind::NotFound);
}
#[test]
fn calls_nest_but_not_forever() {
    let registry = Registry::new();
    registry
        .create_function(info("test.loop", Effect::Read, &[]), Box::new(|cx, _| cx.call("test.loop", json!([]))))
        .unwrap();
    let err = registry.call(&mut (), &Caller::default(), "test.loop", json!([])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Exception);
    assert!(err.message.contains("nest"));
}
#[test]
fn the_host_is_reached_by_type() {
    struct Server {
        hits: u32,
    }
    let registry = Registry::new();
    registry
        .create_function(
            info("test.host", Effect::Write, &[]),
            Box::new(|cx, _| {
                let server = cx.host::<Server>()?;
                server.hits += 1;
                Ok(json!(server.hits))
            }),
        )
        .unwrap();
    let mut server = Server { hits: 0 };
    assert_eq!(registry.call(&mut server, &Caller::default(), "test.host", json!([])).unwrap(), json!(1));
    let err = registry.call(&mut (), &Caller::default(), "test.host", json!([])).unwrap_err();
    assert!(err.message.contains("host"));
}
