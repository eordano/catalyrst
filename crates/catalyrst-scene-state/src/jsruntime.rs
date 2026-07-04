use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::crdt::{decode_batch, CrdtEngine};
use crate::runtime::{RuntimeLimits, ServerTransportConfig};

static V8_INIT: std::sync::Once = std::sync::Once::new();

fn ensure_v8_initialized() {
    V8_INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

#[derive(Debug)]
pub enum Command {
    ClientOpen { index: u32 },

    ClientCrdt { index: u32, body: Vec<u8> },

    ClientClose { index: u32 },

    Shutdown,
}

struct ClientChannel {
    inbound: VecDeque<Vec<u8>>,

    outbound: VecDeque<Vec<u8>>,

    open_delivered: bool,

    closing: bool,
    closed: bool,
}

impl ClientChannel {
    fn new() -> Self {
        Self {
            inbound: VecDeque::new(),
            outbound: VecDeque::new(),
            open_delivered: false,
            closing: false,
            closed: false,
        }
    }
}

struct HostState {
    engine: Arc<Mutex<CrdtEngine>>,

    snapshot: Arc<Mutex<Vec<u8>>>,

    config: Arc<Mutex<ServerTransportConfig>>,

    observer_registered: bool,

    clients: std::collections::BTreeMap<u32, ClientChannel>,

    realm_name: String,

    scene_hash: String,

    client_inbound_max: usize,

    client_outbound_max: usize,
}

type SharedHandles = (
    Arc<Mutex<CrdtEngine>>,
    Arc<Mutex<Vec<u8>>>,
    Arc<Mutex<ServerTransportConfig>>,
);

impl HostState {
    fn with<R>(isolate: &mut v8::Isolate, f: impl FnOnce(&RefCell<HostState>) -> R) -> R {
        let ptr = isolate.get_data(0) as *const RefCell<HostState>;
        let cell = unsafe { &*ptr };
        f(cell)
    }

    fn shared_handles(isolate: &mut v8::Isolate) -> SharedHandles {
        Self::with(isolate, |c| {
            let h = c.borrow();
            (
                Arc::clone(&h.engine),
                Arc::clone(&h.snapshot),
                Arc::clone(&h.config),
            )
        })
    }
}

pub struct SharedState {
    pub engine: Arc<Mutex<CrdtEngine>>,
    pub snapshot: Arc<Mutex<Vec<u8>>>,
    pub config: Arc<Mutex<ServerTransportConfig>>,

    pub outbound: dashmap::DashMap<u32, mpsc::Sender<Vec<u8>>>,
    pub running: AtomicBool,
}

struct Watchdog {
    deadline_ms: AtomicU64,

    tick_active: AtomicBool,

    fired: AtomicBool,

    stopped: AtomicBool,

    epoch: std::time::Instant,

    isolate: parking_lot::Mutex<Option<v8::IsolateHandle>>,
}

impl Watchdog {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

pub struct JsRuntimeHandle {
    pub scene_hash: String,
    pub shared: Arc<SharedState>,
    pub tx: mpsc::UnboundedSender<Command>,
    next_index: AtomicU32,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,

    watchdog: Arc<Watchdog>,

    shutdown_join: std::time::Duration,
}

impl JsRuntimeHandle {
    pub fn next_client_index(&self) -> u32 {
        self.next_index.fetch_add(1, Ordering::SeqCst)
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
        self.shared.running.store(false, Ordering::SeqCst);

        self.watchdog.stopped.store(true, Ordering::SeqCst);
        if let Some(h) = self.watchdog.isolate.lock().as_ref() {
            h.terminate_execution();
        }

        let mut guard = self.join.lock();
        if let Some(j) = guard.as_ref() {
            if j.is_finished() {
                if let Some(j) = guard.take() {
                    let _ = j.join();
                }
                return;
            }
        } else {
            return;
        }
        drop(guard);
        let deadline = std::time::Instant::now() + self.shutdown_join;
        loop {
            {
                let guard = self.join.lock();
                match guard.as_ref() {
                    Some(j) if j.is_finished() => {
                        drop(guard);
                        if let Some(j) = self.join.lock().take() {
                            let _ = j.join();
                        }
                        return;
                    }
                    Some(_) => {}
                    None => return,
                }
            }
            if std::time::Instant::now() >= deadline {
                let _ = self.join.lock().take();
                tracing::warn!(
                    scene = %self.scene_hash,
                    "scene JS thread did not stop within shutdown budget; detaching"
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

impl Drop for JsRuntimeHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn spawn(
    scene_hash: String,
    source: String,
    realm_name: String,
    limits: RuntimeLimits,
    static_crdt: Vec<u8>,
) -> JsRuntimeHandle {
    let engine = Arc::new(Mutex::new(CrdtEngine::with_cap(limits.crdt_max_components)));
    let snapshot = Arc::new(Mutex::new(Vec::new()));

    if !static_crdt.is_empty() {
        let msgs = decode_batch(&static_crdt);
        if !msgs.is_empty() {
            let mut eng = engine.lock();
            eng.apply_batch(&msgs);
            *snapshot.lock() = eng.snapshot();
            tracing::info!(scene = %scene_hash, ops = msgs.len(), "seeded static main.crdt");
        }
    }
    let config = Arc::new(Mutex::new(ServerTransportConfig::default()));
    let shared = Arc::new(SharedState {
        engine: Arc::clone(&engine),
        snapshot: Arc::clone(&snapshot),
        config: Arc::clone(&config),
        outbound: dashmap::DashMap::new(),
        running: AtomicBool::new(true),
    });

    let watchdog = Arc::new(Watchdog {
        deadline_ms: AtomicU64::new(0),
        tick_active: AtomicBool::new(false),
        fired: AtomicBool::new(false),
        stopped: AtomicBool::new(false),
        epoch: std::time::Instant::now(),
        isolate: parking_lot::Mutex::new(None),
    });

    let (tx, rx) = mpsc::unbounded_channel::<Command>();

    let thread_shared = Arc::clone(&shared);
    let thread_watchdog = Arc::clone(&watchdog);
    let thread_hash = scene_hash.clone();
    let join = std::thread::Builder::new()
        .name(format!("scene-js-{scene_hash}"))
        .spawn(move || {
            run_scene_thread(
                thread_hash,
                source,
                realm_name,
                thread_shared,
                thread_watchdog,
                rx,
                limits,
            );
        })
        .expect("spawn scene JS thread");

    JsRuntimeHandle {
        scene_hash,
        shared,
        tx,
        next_index: AtomicU32::new(0),
        join: Mutex::new(Some(join)),
        watchdog,
        shutdown_join: std::time::Duration::from_millis(limits.js_shutdown_join_ms),
    }
}

extern "C" fn near_heap_limit_cb(
    data: *mut c_void,
    current_heap_limit: usize,
    _initial_heap_limit: usize,
) -> usize {
    if !data.is_null() {
        let wd = unsafe { &*(data as *const Watchdog) };
        wd.fired.store(true, Ordering::SeqCst);
        if let Some(h) = wd.isolate.lock().as_ref() {
            h.terminate_execution();
        }
    }

    current_heap_limit + 32 * 1024 * 1024
}

fn run_scene_thread(
    scene_hash: String,
    source: String,
    realm_name: String,
    shared: Arc<SharedState>,
    watchdog: Arc<Watchdog>,
    mut rx: mpsc::UnboundedReceiver<Command>,
    limits: RuntimeLimits,
) {
    ensure_v8_initialized();

    let host: Box<RefCell<HostState>> = Box::new(RefCell::new(HostState {
        engine: Arc::clone(&shared.engine),
        snapshot: Arc::clone(&shared.snapshot),
        config: Arc::clone(&shared.config),
        observer_registered: false,
        clients: std::collections::BTreeMap::new(),
        realm_name,
        scene_hash: scene_hash.clone(),
        client_inbound_max: limits.client_inbound_max,
        client_outbound_max: limits.client_outbound_max,
    }));
    let host_ptr = host.as_ref() as *const RefCell<HostState> as *mut c_void;

    let heap_max = limits.js_heap_limit_mb.saturating_mul(1024 * 1024);
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default().heap_limits(0, heap_max));
    isolate.set_data(0, host_ptr);

    *watchdog.isolate.lock() = Some(isolate.thread_safe_handle());
    let wd_ptr = Arc::as_ptr(&watchdog) as *mut c_void;
    isolate.add_near_heap_limit_callback(near_heap_limit_cb, wd_ptr);

    let wd_thread = {
        let watchdog = Arc::clone(&watchdog);
        std::thread::Builder::new()
            .name(format!("scene-wd-{scene_hash}"))
            .spawn(move || watchdog_loop(watchdog))
            .ok()
    };

    let budget_ms = limits.js_tick_budget_ms.max(1);

    v8::scope!(let handle_scope, isolate);
    let context = v8::Context::new(handle_scope, Default::default());
    let scope = &mut v8::ContextScope::new(handle_scope, context);

    install_globals(scope, context);

    watchdog.arm(budget_ms);
    let eval = eval_scene(scope, &source);
    watchdog.disarm();
    if let Err(e) = eval {
        tracing::error!(scene = %scene_hash, error = %e, "scene eval failed");
        finish(&shared, &watchdog, wd_thread, &scene_hash);
        return;
    }
    if watchdog.was_terminated() {
        tracing::error!(scene = %scene_hash, "scene eval exceeded wall-clock budget; aborting scene");
        finish(&shared, &watchdog, wd_thread, &scene_hash);
        return;
    }

    watchdog.arm(budget_ms);
    let r = call_export(scope, context, "onStart", None);
    watchdog.disarm();
    if let Err(e) = r {
        tracing::warn!(scene = %scene_hash, error = %e, "onStart threw");
    }
    if watchdog.was_terminated() {
        tracing::error!(scene = %scene_hash, "onStart exceeded wall-clock budget; aborting scene");
        finish(&shared, &watchdog, wd_thread, &scene_hash);
        return;
    }
    drain_set_immediate(scope, context);

    let has_on_update = export_is_function(scope, context, "onUpdate");
    if !has_on_update {
        tracing::warn!(
            scene = %scene_hash,
            "scene exports no onUpdate; running as static scene (no game loop)"
        );
    }

    if has_on_update {
        watchdog.arm(budget_ms);
        let _ = call_export(scope, context, "onUpdate", Some(0.0));
        watchdog.disarm();
        if watchdog.was_terminated() {
            tracing::error!(scene = %scene_hash, "onUpdate exceeded wall-clock budget; aborting scene");
            finish(&shared, &watchdog, wd_thread, &scene_hash);
            return;
        }
        drain_set_immediate(scope, context);
    }

    let tick = std::time::Duration::from_micros(1_000_000 / 30);
    let mut last = std::time::Instant::now();

    loop {
        let mut shutdown = false;
        loop {
            match rx.try_recv() {
                Ok(Command::Shutdown) => {
                    shutdown = true;
                    break;
                }
                Ok(Command::ClientOpen { index }) => {
                    host.borrow_mut()
                        .clients
                        .entry(index)
                        .or_insert_with(ClientChannel::new);
                }
                Ok(Command::ClientCrdt { index, body }) => {
                    let mut h = host.borrow_mut();
                    let cap = h.client_inbound_max;
                    if let Some(c) = h.clients.get_mut(&index) {
                        if c.inbound.len() >= cap {
                            tracing::warn!(
                                scene = %scene_hash, index,
                                "client inbound queue overflow; closing client"
                            );
                            c.closing = true;
                        } else {
                            c.inbound.push_back(body);
                        }
                    }
                }
                Ok(Command::ClientClose { index }) => {
                    if let Some(c) = host.borrow_mut().clients.get_mut(&index) {
                        c.closing = true;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    shutdown = true;
                    break;
                }
            }
        }
        if shutdown {
            break;
        }

        watchdog.arm(budget_ms);
        deliver_client_events(scope, context);
        watchdog.disarm();
        if watchdog.was_terminated() {
            tracing::error!(scene = %scene_hash, "observer callback exceeded budget / OOM; aborting scene");
            break;
        }

        if has_on_update {
            let now = std::time::Instant::now();
            let dt = (now - last).as_secs_f64();
            last = now;
            watchdog.arm(budget_ms);
            let r = call_export(scope, context, "onUpdate", Some(dt));
            watchdog.disarm();
            if watchdog.was_terminated() {
                tracing::error!(scene = %scene_hash, "onUpdate exceeded wall-clock budget / OOM; aborting scene");
                break;
            }
            if let Err(e) = r {
                tracing::warn!(scene = %scene_hash, error = %e, "onUpdate threw; stopping loop");
                break;
            }
            drain_set_immediate(scope, context);
        }

        flush_outbound(&host, &shared);

        host.borrow_mut().clients.retain(|_, c| !c.closed);

        if !has_on_update {
            std::thread::sleep(std::time::Duration::from_millis(50));
        } else {
            let elapsed = std::time::Instant::now() - last;
            if elapsed < tick {
                std::thread::sleep(tick - elapsed);
            }
        }
    }

    finish(&shared, &watchdog, wd_thread, &scene_hash);
}

fn finish(
    shared: &SharedState,
    watchdog: &Watchdog,
    wd_thread: Option<std::thread::JoinHandle<()>>,
    scene_hash: &str,
) {
    shared.running.store(false, Ordering::SeqCst);
    watchdog.stopped.store(true, Ordering::SeqCst);

    *watchdog.isolate.lock() = None;
    if let Some(j) = wd_thread {
        let _ = j.join();
    }
    tracing::info!(scene = %scene_hash, "scene JS loop stopped");
}

impl Watchdog {
    fn arm(&self, budget_ms: u64) {
        self.fired.store(false, Ordering::SeqCst);
        self.deadline_ms
            .store(self.now_ms().saturating_add(budget_ms), Ordering::SeqCst);
        self.tick_active.store(true, Ordering::SeqCst);
    }

    fn disarm(&self) {
        self.tick_active.store(false, Ordering::SeqCst);
    }

    fn was_terminated(&self) -> bool {
        let fired = self.fired.swap(false, Ordering::SeqCst);
        if fired {
            if let Some(h) = self.isolate.lock().as_ref() {
                h.cancel_terminate_execution();
            }
        }
        fired
    }
}

fn watchdog_loop(watchdog: Arc<Watchdog>) {
    while !watchdog.stopped.load(Ordering::SeqCst) {
        if watchdog.tick_active.load(Ordering::SeqCst) {
            let now = watchdog.now_ms();
            let deadline = watchdog.deadline_ms.load(Ordering::SeqCst);
            if now >= deadline {
                watchdog.fired.store(true, Ordering::SeqCst);
                if let Some(h) = watchdog.isolate.lock().as_ref() {
                    h.terminate_execution();
                }

                while watchdog.tick_active.load(Ordering::SeqCst)
                    && !watchdog.stopped.load(Ordering::SeqCst)
                {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                continue;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn flush_outbound(host: &RefCell<HostState>, shared: &SharedState) {
    let mut host = host.borrow_mut();
    for (index, ch) in host.clients.iter_mut() {
        if ch.outbound.is_empty() {
            continue;
        }
        if let Some(sender) = shared.outbound.get(index) {
            while let Some(body) = ch.outbound.pop_front() {
                let _ = sender.try_send(crate::runtime::frame_crdt(&body));
            }
        } else {
            ch.outbound.clear();
        }
    }
}

fn read_uint8array(_scope: &mut v8::PinScope, val: v8::Local<v8::Value>) -> Option<Vec<u8>> {
    let view = v8::Local::<v8::ArrayBufferView>::try_from(val).ok()?;
    let len = view.byte_length();
    let mut out = vec![0u8; len];
    view.copy_contents(&mut out);
    Some(out)
}

fn make_uint8array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
) -> v8::Local<'s, v8::Uint8Array> {
    let store = v8::ArrayBuffer::new_backing_store_from_vec(bytes.to_vec()).make_shared();
    let ab = v8::ArrayBuffer::with_backing_store(scope, &store);
    v8::Uint8Array::new(scope, ab, 0, bytes.len()).unwrap()
}

fn str<'s>(scope: &mut v8::PinScope<'s, '_>, s: &str) -> v8::Local<'s, v8::String> {
    v8::String::new(scope, s).unwrap()
}

fn set_prop(
    scope: &mut v8::PinScope,
    obj: v8::Local<v8::Object>,
    key: &str,
    value: v8::Local<v8::Value>,
) {
    let k = str(scope, key).into();
    obj.set(scope, k, value);
}

fn set_fn(
    scope: &mut v8::PinScope,
    obj: v8::Local<v8::Object>,
    name: &str,
    cb: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let f = v8::Function::new(scope, cb).unwrap();
    let k = str(scope, name).into();
    obj.set(scope, k, f.into());
}

fn op_crdt_send_to_renderer(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let (engine, snapshot, _cfg) = HostState::shared_handles(scope);

    if let Ok(obj) = v8::Local::<v8::Object>::try_from(args.get(0)) {
        let key = str(scope, "data").into();
        if let Some(data_val) = obj.get(scope, key) {
            if let Some(bytes) = read_uint8array(scope, data_val) {
                if !bytes.is_empty() {
                    let msgs = decode_batch(&bytes);
                    engine.lock().apply_batch(&msgs);
                }
            }
        }
    }

    {
        let eng = engine.lock();
        *snapshot.lock() = eng.snapshot();
    }

    let result = v8::Object::new(scope);
    let empty = v8::Array::new(scope, 0);
    set_prop(scope, result, "data", empty.into());
    rv.set(result.into());
}

fn op_crdt_get_state(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let (_engine, snapshot, _cfg) = HostState::shared_handles(scope);
    let snap = snapshot.lock().clone();
    let result = v8::Object::new(scope);
    let has = !snap.is_empty();
    let has_v = v8::Boolean::new(scope, has);
    set_prop(scope, result, "hasEntities", has_v.into());
    let arr = v8::Array::new(scope, if has { 1 } else { 0 });
    if has {
        let ua = make_uint8array(scope, &snap);
        arr.set_index(scope, 0, ua.into());
    }
    set_prop(scope, result, "data", arr.into());
    rv.set(result.into());
}

fn op_is_server(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let result = v8::Object::new(scope);
    let t = v8::Boolean::new(scope, true);
    set_prop(scope, result, "isServer", t.into());
    rv.set(result.into());
}

fn op_send_batch(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let result = v8::Object::new(scope);
    let arr = v8::Array::new(scope, 0);
    set_prop(scope, result, "events", arr.into());
    rv.set(result.into());
}

fn op_get_realm(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let realm_name = HostState::with(scope, |c| c.borrow().realm_name.clone());
    let info = v8::Object::new(scope);
    let f = v8::Boolean::new(scope, false);
    set_prop(scope, info, "isPreview", f.into());
    let name = str(scope, &realm_name).into();
    set_prop(scope, info, "realmName", name);
    let result = v8::Object::new(scope);
    set_prop(scope, result, "realmInfo", info.into());
    rv.set(result.into());
}

fn op_get_scene_information(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let hash = HostState::with(scope, |c| c.borrow().scene_hash.clone());
    let result = v8::Object::new(scope);
    let urn = str(scope, &hash).into();
    set_prop(scope, result, "urn", urn);
    let base = str(scope, "").into();
    set_prop(scope, result, "baseUrl", base);
    let content = v8::Array::new(scope, 0);
    set_prop(scope, result, "content", content.into());
    let meta = str(scope, "{}").into();
    set_prop(scope, result, "metadataJson", meta);
    rv.set(result.into());
}

fn op_read_file(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let result = v8::Object::new(scope);
    let ua = make_uint8array(scope, &[]);
    set_prop(scope, result, "content", ua.into());
    let h = str(scope, "").into();
    set_prop(scope, result, "hash", h);
    rv.set(result.into());
}

fn op_empty_promise(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let promise = resolver.get_promise(scope);
    let result = v8::Object::new(scope);
    resolver.resolve(scope, result.into());
    rv.set(promise.into());
}

fn op_register_scene(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    if let Ok(cfg) = v8::Local::<v8::Object>::try_from(args.get(0)) {
        let reserved = get_u32_prop(scope, cfg, "reservedLocalEntities").unwrap_or(512);
        let (server_limit, client_limit) = {
            let key = str(scope, "networkEntitiesLimit").into();
            if let Some(nv) = cfg.get(scope, key) {
                if let Ok(no) = v8::Local::<v8::Object>::try_from(nv) {
                    (
                        get_u32_prop(scope, no, "serverLimit").unwrap_or(512),
                        get_u32_prop(scope, no, "clientLimit").unwrap_or(512),
                    )
                } else {
                    (512, 512)
                }
            } else {
                (512, 512)
            }
        };
        HostState::with(scope, |c| {
            let h = c.borrow();
            *h.config.lock() = ServerTransportConfig {
                reserved_local_entities: reserved,
                server_network_entities_limit: server_limit,
                client_network_entities_limit: client_limit,
            };
            drop(h);
            c.borrow_mut().observer_registered = true;
        });
    }

    let observer = args.get(1);
    if observer.is_function() {
        let ctx = scope.get_current_context();
        let global = ctx.global(scope);
        let key = str(scope, "__observer").into();
        global.set(scope, key, observer);
    }
}

fn op_update_crdt_state(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    if let Some(bytes) = read_uint8array(scope, args.get(0)) {
        let (_engine, snapshot, _cfg) = HostState::shared_handles(scope);
        *snapshot.lock() = bytes;
    }
}

fn op_console_log(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let mut parts = Vec::new();
    for i in 0..args.length() {
        let s = args
            .get(i)
            .to_string(scope)
            .map(|s| s.to_rust_string_lossy(scope))
            .unwrap_or_default();
        parts.push(s);
    }
    tracing::info!(target: "scene_console", "{}", parts.join(" "));
}

fn op_set_immediate(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let fn_arg = args.get(0);
    if !fn_arg.is_function() {
        return;
    }
    let ctx = scope.get_current_context();
    let global = ctx.global(scope);
    let key = str(scope, "__setImmediate").into();
    let list = match global.get(scope, key) {
        Some(v) if v.is_array() => v8::Local::<v8::Array>::try_from(v).unwrap(),
        _ => {
            let arr = v8::Array::new(scope, 0);
            global.set(scope, key, arr.into());
            arr
        }
    };
    let len = list.length();
    list.set_index(scope, len, fn_arg);
}

fn op_require(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args
        .get(0)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();
    let module = name.strip_prefix("~system/").unwrap_or(&name);
    let obj = build_system_module(scope, module);
    match obj {
        Some(o) => rv.set(o.into()),
        None => {
            let msg = str(scope, &format!("Unknown module {name}"));
            let exc = v8::Exception::error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

fn op_restricted(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let msg = str(scope, "Disabled on server");
    let exc = v8::Exception::error(scope, msg);
    scope.throw_exception(exc);
}

fn get_u32_prop(scope: &mut v8::PinScope, obj: v8::Local<v8::Object>, key: &str) -> Option<u32> {
    let k = str(scope, key).into();
    let v = obj.get(scope, k)?;
    v.uint32_value(scope)
}

fn build_system_module<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    module: &str,
) -> Option<v8::Local<'s, v8::Object>> {
    let obj = v8::Object::new(scope);
    match module {
        "EngineApi" => {
            set_fn(scope, obj, "crdtSendToRenderer", op_crdt_send_to_renderer);
            set_fn(scope, obj, "crdtGetState", op_crdt_get_state);
            set_fn(scope, obj, "isServer", op_is_server);
            set_fn(scope, obj, "sendBatch", op_send_batch);
        }
        "Runtime" => {
            set_fn(scope, obj, "getRealm", op_get_realm);
            set_fn(scope, obj, "getSceneInformation", op_get_scene_information);
            set_fn(scope, obj, "readFile", op_read_file);
        }
        "UserIdentity" => {
            set_fn(scope, obj, "getUserData", op_empty_promise);
        }
        "SignedFetch" => {
            set_fn(scope, obj, "getHeaders", op_empty_promise);
        }

        "RestrictedActions" => {
            for m in [
                "movePlayerTo",
                "teleportTo",
                "triggerEmote",
                "triggerSceneEmote",
                "openExternalUrl",
                "openNftDialog",
                "changeRealm",
            ] {
                set_fn(scope, obj, m, op_empty_promise);
            }
        }
        "EnvironmentApi" | "EnvironmentAPI" => {}

        _ => {}
    }
    Some(obj)
}

fn install_globals(scope: &mut v8::PinScope, context: v8::Local<v8::Context>) {
    let global = context.global(scope);

    let module = v8::Object::new(scope);
    let exports = v8::Object::new(scope);
    set_prop(scope, module, "exports", exports.into());
    set_prop(scope, global, "module", module.into());
    set_prop(scope, global, "exports", exports.into());

    let console = v8::Object::new(scope);
    for m in ["log", "info", "debug", "trace", "warning", "error"] {
        set_fn(scope, console, m, op_console_log);
    }
    set_prop(scope, global, "console", console.into());

    set_fn(scope, global, "require", op_require);
    set_fn(scope, global, "setImmediate", op_set_immediate);
    set_fn(scope, global, "registerScene", op_register_scene);
    set_fn(scope, global, "updateCRDTState", op_update_crdt_state);

    set_fn(scope, global, "fetch", op_restricted);
    set_fn(scope, global, "WebSocket", op_restricted);

    let g: v8::Local<v8::Value> = global.into();
    set_prop(scope, global, "self", g);
    set_prop(scope, global, "global", g);
}

fn eval_scene(scope: &mut v8::PinScope, source: &str) -> Result<(), String> {
    v8::tc_scope!(let tc, scope);
    let code = v8::String::new(tc, source).ok_or("source too large")?;
    let script = v8::Script::compile(tc, code, None).ok_or_else(|| caught(tc))?;
    if script.run(tc).is_none() {
        return Err(caught(tc));
    }
    Ok(())
}

fn caught(tc: &mut v8::PinnedRef<v8::TryCatch<v8::HandleScope>>) -> String {
    tc.exception()
        .and_then(|e| e.to_string(tc))
        .map(|s| s.to_rust_string_lossy(tc))
        .unwrap_or_else(|| "unknown JS error".into())
}

fn export_is_function(
    scope: &mut v8::PinScope,
    context: v8::Local<v8::Context>,
    name: &str,
) -> bool {
    let global = context.global(scope);
    let exports_key = str(scope, "exports").into();
    let Some(exports) = global.get(scope, exports_key) else {
        return false;
    };
    let Ok(exports) = v8::Local::<v8::Object>::try_from(exports) else {
        return false;
    };
    let fk = str(scope, name).into();
    matches!(exports.get(scope, fk), Some(v) if v.is_function())
}

fn call_export(
    scope: &mut v8::PinScope,
    context: v8::Local<v8::Context>,
    name: &str,
    dt: Option<f64>,
) -> Result<(), String> {
    let global = context.global(scope);
    let exports_key = str(scope, "exports").into();
    let exports = global.get(scope, exports_key).ok_or("no exports")?;
    let exports = v8::Local::<v8::Object>::try_from(exports).map_err(|_| "exports not object")?;
    let fk = str(scope, name).into();
    let f = match exports.get(scope, fk) {
        Some(v) if v.is_function() => v8::Local::<v8::Function>::try_from(v).unwrap(),
        _ => return Ok(()),
    };

    v8::tc_scope!(let tc, scope);
    let recv: v8::Local<v8::Value> = exports.into();
    let result = if let Some(dt) = dt {
        let arg = v8::Number::new(tc, dt).into();
        f.call(tc, recv, &[arg])
    } else {
        f.call(tc, recv, &[])
    };
    if result.is_none() {
        return Err(caught(tc));
    }

    tc.perform_microtask_checkpoint();
    Ok(())
}

fn drain_set_immediate(scope: &mut v8::PinScope, context: v8::Local<v8::Context>) {
    let global = context.global(scope);
    let key = str(scope, "__setImmediate").into();
    let list = match global.get(scope, key) {
        Some(v) if v.is_array() => v8::Local::<v8::Array>::try_from(v).unwrap(),
        _ => return,
    };
    let len = list.length();
    let recv: v8::Local<v8::Value> = v8::undefined(scope).into();
    for i in 0..len {
        if let Some(item) = list.get_index(scope, i) {
            if item.is_function() {
                let f = v8::Local::<v8::Function>::try_from(item).unwrap();
                v8::tc_scope!(let tc, scope);
                let _ = f.call(tc, recv, &[]);
            }
        }
    }

    let fresh = v8::Array::new(scope, 0);
    global.set(scope, key, fresh.into());
}

fn deliver_client_events(scope: &mut v8::PinScope, context: v8::Local<v8::Context>) {
    let (registered, to_open, to_close) = HostState::with(scope, |c| {
        let h = c.borrow();
        if !h.observer_registered {
            return (false, Vec::new(), Vec::new());
        }
        let mut to_open: Vec<u32> = Vec::new();
        let mut to_close: Vec<u32> = Vec::new();
        for (index, ch) in h.clients.iter() {
            if !ch.open_delivered && !ch.closing && !ch.closed {
                to_open.push(*index);
            }
            if ch.closing && !ch.closed {
                to_close.push(*index);
            }
        }
        (true, to_open, to_close)
    });
    if !registered {
        return;
    }

    let global = context.global(scope);
    let obs_key = str(scope, "__observer").into();
    let observer = match global.get(scope, obs_key) {
        Some(v) if v.is_function() => v8::Local::<v8::Function>::try_from(v).unwrap(),
        _ => return,
    };
    let recv: v8::Local<v8::Value> = v8::undefined(scope).into();

    for index in to_open {
        HostState::with(scope, |c| {
            if let Some(ch) = c.borrow_mut().clients.get_mut(&index) {
                ch.open_delivered = true;
            }
        });
        let client_obj = build_client_object(scope, index);
        let event = v8::Object::new(scope);
        let ty = str(scope, "open").into();
        set_prop(scope, event, "type", ty);
        let id = str(scope, &index.to_string()).into();
        set_prop(scope, event, "clientId", id);
        set_prop(scope, event, "client", client_obj.into());

        v8::tc_scope!(let tc, scope);
        let _ = observer.call(tc, recv, &[event.into()]);
    }

    for index in to_close {
        let event = v8::Object::new(scope);
        let ty = str(scope, "close").into();
        set_prop(scope, event, "type", ty);
        let id = str(scope, &index.to_string()).into();
        set_prop(scope, event, "clientId", id);
        {
            v8::tc_scope!(let tc, scope);
            let _ = observer.call(tc, recv, &[event.into()]);
        }

        let (engine, snapshot, config) = HostState::shared_handles(scope);
        let (start, size) = config.lock().range_for_client(index);
        let deletes = engine.lock().reclaim_range(start, size);
        if !deletes.is_empty() {
            let body = crate::crdt::encode_batch(&deletes);
            *snapshot.lock() = engine.lock().snapshot();
            HostState::with(scope, |c| {
                let mut h = c.borrow_mut();
                for (other, ch) in h.clients.iter_mut() {
                    if *other != index && !ch.closed {
                        ch.outbound.push_back(body.clone());
                    }
                }
            });
        }
        HostState::with(scope, |c| {
            if let Some(ch) = c.borrow_mut().clients.get_mut(&index) {
                ch.closed = true;
            }
        });
    }
}

fn build_client_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: u32,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    let idx_key = str(scope, "__index").into();
    let idx_val = v8::Integer::new_from_unsigned(scope, index).into();
    obj.set(scope, idx_key, idx_val);
    set_fn(scope, obj, "sendCrdtMessage", op_client_send);
    set_fn(scope, obj, "getMessages", op_client_get_messages);
    obj
}

fn this_index(scope: &mut v8::PinScope, args: &v8::FunctionCallbackArguments) -> Option<u32> {
    let this = args.this();
    let key = str(scope, "__index").into();
    this.get(scope, key)?.uint32_value(scope)
}

fn op_client_send(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(index) = this_index(scope, &args) else {
        return;
    };
    let Some(bytes) = read_uint8array(scope, args.get(0)) else {
        return;
    };
    if bytes.is_empty() {
        return;
    }

    let (engine, snapshot, _cfg) = HostState::shared_handles(scope);
    {
        let msgs = decode_batch(&bytes);
        let mut eng = engine.lock();
        eng.apply_batch(&msgs);
        *snapshot.lock() = eng.snapshot();
    }
    HostState::with(scope, |c| {
        let mut h = c.borrow_mut();
        let cap = h.client_outbound_max;
        if let Some(ch) = h.clients.get_mut(&index) {
            if ch.outbound.len() >= cap {
                ch.outbound.pop_front();
            }

            ch.outbound.push_back(bytes);
        }
    });
}

fn op_client_get_messages(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(index) = this_index(scope, &args) else {
        let empty = v8::Array::new(scope, 0);
        rv.set(empty.into());
        return;
    };
    let msgs: Vec<Vec<u8>> = HostState::with(scope, |c| {
        c.borrow_mut()
            .clients
            .get_mut(&index)
            .map(|ch| ch.inbound.drain(..).collect())
            .unwrap_or_default()
    });
    let arr = v8::Array::new(scope, msgs.len() as i32);
    for (i, m) in msgs.iter().enumerate() {
        let ua = make_uint8array(scope, m);
        arr.set_index(scope, i as u32, ua.into());
    }
    rv.set(arr.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::{decode_batch, encode_batch, CrdtMessage};
    use std::time::Duration;

    fn put(entity: u32, comp: u32, ts: u32, data: &[u8]) -> CrdtMessage {
        CrdtMessage::Put {
            entity,
            component_id: comp,
            timestamp: ts,
            data: data.to_vec(),
        }
    }

    fn wait_for<F: Fn() -> bool>(pred: F) -> bool {
        for _ in 0..200 {
            if pred() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        pred()
    }

    #[test]
    fn scene_onstart_writes_to_engine() {
        let batch = encode_batch(&[put(600, 1, 1, &[42])]);
        let js = format!(
            r#"
            var EngineApi = require('~system/EngineApi');
            module.exports.onStart = async function () {{
              var bytes = new Uint8Array({:?});
              await EngineApi.crdtSendToRenderer({{ data: bytes }});
            }};
            registerScene(
              {{ reservedLocalEntities: 512, networkEntitiesLimit: {{ serverLimit: 512, clientLimit: 512 }} }},
              function (ev) {{}}
            );
            "#,
            batch
        );
        let handle = spawn(
            "test-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
        );
        let ok = wait_for(|| {
            let eng = handle.shared.engine.lock();
            eng.component_count() == 1
        });
        assert!(ok, "scene onStart should have written one component");
        let snap = handle.shared.snapshot.lock().clone();
        assert_eq!(decode_batch(&snap), vec![put(600, 1, 1, &[42])]);
    }

    #[test]
    fn scene_relays_client_messages_via_observer() {
        let js = r#"
            var clients = {};
            registerScene(
              { reservedLocalEntities: 512, networkEntitiesLimit: { serverLimit: 512, clientLimit: 512 } },
              function (ev) {
                if (ev.type === 'open') { clients[ev.clientId] = ev.client; }
                if (ev.type === 'close') { delete clients[ev.clientId]; }
              }
            );
            module.exports.onUpdate = async function (dt) {
              for (var id in clients) {
                var msgs = clients[id].getMessages();
                for (var i = 0; i < msgs.length; i++) {
                  // Echo the exact bytes back to the same client.
                  clients[id].sendCrdtMessage(msgs[i]);
                }
              }
            };
        "#
        .to_string();

        let handle = spawn(
            "echo-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
        );

        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
        let index = handle.next_client_index();
        handle.shared.outbound.insert(index, tx);
        handle.tx.send(Command::ClientOpen { index }).unwrap();

        std::thread::sleep(Duration::from_millis(60));

        let body = encode_batch(&[put(1100, 1, 7, &[1, 2, 3])]);
        handle
            .tx
            .send(Command::ClientCrdt {
                index,
                body: body.clone(),
            })
            .unwrap();

        let frame = wait_for_recv(&mut rx);
        assert!(frame.is_some(), "expected an echoed Crdt frame");
        let frame = frame.unwrap();

        assert_eq!(frame[0], crate::protocol::MessageType::Crdt as u8);
        assert_eq!(decode_batch(&frame[1..]), vec![put(1100, 1, 7, &[1, 2, 3])]);
    }

    #[test]
    fn client_close_reclaims_range() {
        let js = r#"
            registerScene(
              { reservedLocalEntities: 512, networkEntitiesLimit: { serverLimit: 512, clientLimit: 512 } },
              function (ev) {}
            );
            module.exports.onStart = async function () {};
        "#
        .to_string();
        let handle = spawn(
            "close-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
        );

        let (tx, _rx) = mpsc::channel::<Vec<u8>>(64);
        let index = handle.next_client_index();
        handle.shared.outbound.insert(index, tx);
        handle.tx.send(Command::ClientOpen { index }).unwrap();
        std::thread::sleep(Duration::from_millis(60));

        let body = encode_batch(&[put(1100, 1, 1, &[1])]);
        handle.tx.send(Command::ClientCrdt { index, body }).unwrap();

        handle.shared.engine.lock().apply(&put(1100, 1, 1, &[1]));
        assert!(handle.shared.engine.lock().component_count() >= 1);

        handle.tx.send(Command::ClientClose { index }).unwrap();
        let reclaimed = wait_for(|| handle.shared.engine.lock().component_count() == 0);
        assert!(reclaimed, "close should reclaim the client's network range");
    }

    fn wait_for_recv(rx: &mut mpsc::Receiver<Vec<u8>>) -> Option<Vec<u8>> {
        for _ in 0..200 {
            if let Ok(v) = rx.try_recv() {
                return Some(v);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn infinite_onupdate_is_terminated_and_shutdown_does_not_hang() {
        let js = r#"
            registerScene(
              { reservedLocalEntities: 512, networkEntitiesLimit: { serverLimit: 512, clientLimit: 512 } },
              function (ev) {}
            );
            module.exports.onUpdate = function (dt) {
              // Spin forever — must be force-terminated by the watchdog.
              while (true) {}
            };
        "#
        .to_string();
        let limits = RuntimeLimits {
            js_tick_budget_ms: 100,
            js_shutdown_join_ms: 2000,
            ..RuntimeLimits::default()
        };
        let handle = spawn(
            "spin-scene".into(),
            js,
            "dcl-test".into(),
            limits,
            Vec::new(),
        );

        let stopped = wait_for(|| !handle.shared.running.load(Ordering::SeqCst));
        assert!(
            stopped,
            "watchdog should have terminated the infinite onUpdate"
        );

        let start = std::time::Instant::now();
        handle.shutdown();
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "shutdown() must not block on a wedged scene"
        );
    }

    #[test]
    fn crdt_cap_rejects_new_cells() {
        use crate::crdt::{ApplyResult, CrdtEngine};
        let mut e = CrdtEngine::with_cap(2);
        assert_eq!(e.apply(&put(1, 1, 1, b"a")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 2, 1, b"b")), ApplyResult::Applied);

        assert_eq!(e.apply(&put(1, 3, 1, b"c")), ApplyResult::Ignored);

        assert_eq!(e.apply(&put(1, 1, 2, b"a2")), ApplyResult::Applied);
        assert_eq!(e.component_count(), 2);
    }
}
