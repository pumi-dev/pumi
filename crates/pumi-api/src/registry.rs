use crate::meta::{Effect, Function, FunctionInfo};
use crate::{Error, Result};
use serde_json::Value;
use std::any::{Any, type_name};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::OnceLock;
#[cfg(not(target_family = "wasm"))]
#[linkme::distributed_slice]
pub static FUNCTIONS: [Function];
#[cfg(target_family = "wasm")]
pub static FUNCTIONS: &[Function] = &[];
pub type Handler = fn(&mut Cx<'_>, Value) -> Result<Value>;
pub const MAX_DEPTH: u32 = 64;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caller {
    pub channel: u64,
    pub client: Option<String>,
    pub tab: Option<u64>,
    pub source: Option<String>,
    pub api_level: Option<u32>,
}
pub type DynHandler = Box<dyn Fn(&mut Cx<'_>, Value) -> Result<Value>>;
/// A function this registry does not hold itself: the host hands it to the core.
pub type Forward = Rc<dyn Fn(&mut Cx<'_>, &str, Value) -> Result<Value>>;
pub struct DynFunction {
    pub info: FunctionInfo,
    pub handler: DynHandler,
}
#[derive(Clone)]
enum Entry {
    Core(&'static Function),
    Dyn(Rc<DynFunction>),
    Forwarded(Forward),
}
impl Entry {
    fn effect(&self) -> Effect {
        match self {
            Entry::Core(function) => function.effect,
            Entry::Dyn(function) => function.info.effect,
            Entry::Forwarded(_) => Effect::Forward,
        }
    }
}
pub type Wrapper = Rc<dyn Fn(&mut Cx<'_>, Value) -> Result<Value>>;
struct Wrap {
    id: u64,
    wrapper: Wrapper,
}
pub struct Registry {
    dynamic: RefCell<BTreeMap<String, Rc<DynFunction>>>,
    wraps: RefCell<HashMap<String, Vec<Wrap>>>,
    next_wrap: Cell<u64>,
    frames: RefCell<Vec<(String, usize)>>,
    forward: RefCell<Option<Forward>>,
}
impl Default for Registry {
    fn default() -> Self {
        Registry::new()
    }
}
impl Registry {
    pub fn new() -> Registry {
        Registry {
            dynamic: RefCell::new(BTreeMap::new()),
            wraps: RefCell::new(HashMap::new()),
            next_wrap: Cell::new(0),
            frames: RefCell::new(Vec::new()),
            forward: RefCell::new(None),
        }
    }
    /// Where a function this registry does not hold goes: `pumi-host` hands it to the core.
    pub fn set_forward(&self, forward: Forward) {
        *self.forward.borrow_mut() = Some(forward);
    }
    pub fn wrap_function(&self, name: &str, wrapper: Wrapper) -> Result<u64> {
        if self.lookup(name).is_none() {
            return Err(Error::not_found(format!("no function `{name}`")));
        }
        let id = self.next_wrap.get() + 1;
        self.next_wrap.set(id);
        self.wraps.borrow_mut().entry(name.to_string()).or_default().insert(0, Wrap { id, wrapper });
        Ok(id)
    }
    pub fn is_wrapped(&self, name: &str) -> bool {
        self.wraps.borrow().get(name).is_some_and(|list| !list.is_empty())
    }
    pub fn unwrap_function(&self, id: u64) -> Result<()> {
        let mut wraps = self.wraps.borrow_mut();
        for list in wraps.values_mut() {
            if let Some(at) = list.iter().position(|w| w.id == id) {
                list.remove(at);
                return Ok(());
            }
        }
        Err(Error::not_found(format!("no wrapper {id}")))
    }
    pub fn wrappers(&self, name: &str) -> Vec<u64> {
        self.wraps.borrow().get(name).map(|list| list.iter().map(|w| w.id).collect()).unwrap_or_default()
    }
    pub fn core(name: &str) -> Option<&'static Function> {
        core_index().get(name).copied()
    }
    pub fn core_functions() -> Vec<&'static Function> {
        let mut functions: Vec<_> = core_index().values().copied().collect();
        functions.sort_by_key(|function| function.name);
        functions
    }
    pub fn create_function(&self, info: FunctionInfo, handler: DynHandler) -> Result<()> {
        info.validate()?;
        if Registry::core(&info.name).is_some() {
            return Err(Error::invalid_params(format!("`{}` is a core function", info.name)));
        }
        let mut dynamic = self.dynamic.borrow_mut();
        if dynamic.contains_key(&info.name) {
            return Err(Error::invalid_params(format!("`{}` already exists; delete it first", info.name)));
        }
        dynamic.insert(info.name.clone(), Rc::new(DynFunction { info, handler }));
        Ok(())
    }
    pub fn del_function(&self, name: &str) -> Result<()> {
        if Registry::core(name).is_some() {
            return Err(Error::invalid_params(format!("`{name}` is a core function")));
        }
        match self.dynamic.borrow_mut().remove(name) {
            Some(_) => Ok(()),
            None => Err(Error::not_found(format!("no function `{name}`"))),
        }
    }
    pub fn function_info(&self, name: &str) -> Option<FunctionInfo> {
        match self.lookup(name)? {
            Entry::Core(function) => Some(function.info()),
            Entry::Dyn(function) => Some(function.info.clone()),
            Entry::Forwarded(_) => None,
        }
    }
    pub fn functions(&self) -> Vec<FunctionInfo> {
        let mut all: Vec<FunctionInfo> =
            core_index().values().filter(|function| !function.internal).map(|function| function.info()).collect();
        all.extend(self.dynamic.borrow().values().map(|function| function.info.clone()));
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }
    pub fn internal_functions() -> Vec<FunctionInfo> {
        let mut all: Vec<FunctionInfo> =
            core_index().values().filter(|function| function.internal).map(|function| function.info()).collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }
    pub fn call(&self, host: &mut dyn Any, caller: &Caller, name: &str, args: Value) -> Result<Value> {
        let mut cx = Cx { registry: self, host, caller, depth: 0, in_op: false };
        if let Some(level) = caller.api_level {
            crate::level::check(level)?;
        }
        cx.call(name, args)
    }
    pub fn with_cx<R>(&self, host: &mut dyn Any, caller: &Caller, f: impl FnOnce(&mut Cx<'_>) -> R) -> R {
        let mut cx = Cx { registry: self, host, caller, depth: 0, in_op: false };
        f(&mut cx)
    }
    fn lookup(&self, name: &str) -> Option<Entry> {
        if let Some(function) = Registry::core(name) {
            return Some(Entry::Core(function));
        }
        if let Some(function) = self.dynamic.borrow().get(name).cloned() {
            return Some(Entry::Dyn(function));
        }
        self.forward.borrow().clone().map(Entry::Forwarded)
    }
}
fn core_index() -> &'static HashMap<&'static str, &'static Function> {
    static INDEX: OnceLock<HashMap<&'static str, &'static Function>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::with_capacity(FUNCTIONS.len());
        for function in FUNCTIONS.iter() {
            if let Some(previous) = index.insert(function.name, function) {
                panic!(
                    "two core functions are named `{}`: in {} and {}",
                    function.name, previous.module, function.module
                );
            }
        }
        index
    })
}
pub struct Cx<'a> {
    registry: &'a Registry,
    host: &'a mut dyn Any,
    caller: &'a Caller,
    depth: u32,
    in_op: bool,
}
impl<'a> Cx<'a> {
    pub fn caller(&self) -> &Caller {
        self.caller
    }
    pub fn registry(&self) -> &'a Registry {
        self.registry
    }
    pub fn host<T: Any>(&mut self) -> Result<&mut T> {
        self.host
            .downcast_mut::<T>()
            .ok_or_else(|| Error::exception(format!("this function needs a {} host", type_name::<T>())))
    }
    pub fn as_caller<R>(&mut self, caller: &Caller, f: impl FnOnce(&mut Cx<'_>) -> R) -> R {
        let mut cx =
            Cx { registry: self.registry, host: &mut *self.host, caller, depth: self.depth, in_op: self.in_op };
        f(&mut cx)
    }
    pub fn call(&mut self, name: &str, args: Value) -> Result<Value> {
        if self.depth >= MAX_DEPTH {
            return Err(Error::exception(format!("calls nest deeper than {MAX_DEPTH}")));
        }
        let entry = self.registry.lookup(name).ok_or_else(|| Error::not_found(format!("no function `{name}`")))?;
        let write = entry.effect() == Effect::Write && !self.in_op;
        let was_in_op = self.in_op;
        self.depth += 1;
        self.in_op |= write;
        let result = self.layer(name, &entry, 0, args);
        self.depth -= 1;
        self.in_op = was_in_op;
        result
    }
}
impl Cx<'_> {
    fn layer(&mut self, name: &str, entry: &Entry, at: usize, args: Value) -> Result<Value> {
        let wrapper = self.registry.wraps.borrow().get(name).and_then(|list| list.get(at)).map(|w| w.wrapper.clone());
        match wrapper {
            Some(wrapper) => {
                self.registry.frames.borrow_mut().push((name.to_string(), at + 1));
                let result = wrapper(self, args);
                self.registry.frames.borrow_mut().pop();
                result
            }
            None => match entry {
                Entry::Core(function) => (function.call)(self, args),
                Entry::Dyn(function) => match check_shape(&function.info, &args) {
                    Ok(()) => (function.handler)(self, args),
                    Err(err) => Err(err),
                },
                Entry::Forwarded(forward) => forward(self, name, args),
            },
        }
    }
    pub fn call_orig(&mut self, args: Value) -> Result<Value> {
        let (name, at) = self
            .registry
            .frames
            .borrow()
            .last()
            .cloned()
            .ok_or_else(|| Error::invalid_params("call_orig is only for a wrapper, while it runs"))?;
        let entry = self.registry.lookup(&name).ok_or_else(|| Error::not_found(format!("no function `{name}`")))?;
        self.depth += 1;
        let result = self.layer(&name, &entry, at, args);
        self.depth -= 1;
        result
    }
}
fn check_shape(info: &FunctionInfo, args: &Value) -> Result<()> {
    let names = info.param_names();
    match args {
        Value::Null => Ok(()),
        Value::Array(list) if list.len() <= names.len() => Ok(()),
        Value::Array(list) => {
            Err(Error::invalid_params(format!("{} takes {} arguments, got {}", info.name, names.len(), list.len())))
        }
        Value::Object(map) => match map.keys().find(|key| !names.contains(key)) {
            Some(unknown) => Err(Error::invalid_params(format!("{} has no argument `{unknown}`", info.name))),
            None => Ok(()),
        },
        _ => Err(Error::invalid_params(format!("{}: arguments are an array or an object", info.name))),
    }
}
