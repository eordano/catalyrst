//! Server-side SDK7 JavaScript runtime — runs the scene's own compiled
//! `bin/game.js` headlessly inside V8.
//!
//! This is the real port of `src/logic/scene-runtime/*` + `src/adapters/scene.ts`
//! upstream. It embeds V8 (via the `v8` crate — the same `rusty_v8` engine
//! `deno_core` wraps; see the crate README "V8 under Nix" note for the offline
//! build recipe) and reproduces:
//!
//! - **`sandbox.ts` / `sdk7-runtime.ts`**: a custom global execution context
//!   exposing `module`/`exports`, `console`, `require('~system/*')`,
//!   `setImmediate`, `registerScene` and `updateCRDTState`. The scene source is
//!   evaluated with `new Function('globalThis', 'with (globalThis) {<code>}')`
//!   over a Proxy so the scene's free identifiers resolve against our context —
//!   exactly the `customEvalSdk7` trick.
//! - **`apis.ts`**: the `~system/*` host module surface — `EngineApi`
//!   (`crdtSendToRenderer`, `crdtGetState`, `isServer`→true, `sendBatch`),
//!   `Runtime` (`getRealm`, `getSceneInformation`, `readFile`), and the no-op
//!   `UserIdentity` / `SignedFetch`. `fetch` and `WebSocket` throw, matching
//!   `createWsFetchRuntime`.
//! - **`scene.ts`**: the `onStart` + 30 Hz `onUpdate` game loop, the
//!   `registerScene(config, observer)` wiring, the per-client
//!   `sendCrdtMessage`/`getMessages` channel, and the authoritative
//!   `crdtState: Uint8Array` snapshot that newly-joined clients receive.
//!
//! # Threading
//!
//! A V8 isolate is single-threaded. The whole scene loop therefore runs on one
//! dedicated OS thread ([`JsRuntime::spawn`]). The async server interacts with
//! it purely through [`Command`]s on an MPSC channel plus a shared
//! [`SharedState`] (mutex-guarded) that lets the WS tasks read the current
//! snapshot synchronously for the `Init` frame without round-tripping to the JS
//! thread.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::crdt::{decode_batch, CrdtEngine};
use crate::runtime::{RuntimeLimits, ServerTransportConfig};

/// One global V8 init guard for the whole process.
static V8_INIT: std::sync::Once = std::sync::Once::new();

fn ensure_v8_initialized() {
    V8_INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

/// Commands sent from the async server into the dedicated JS thread.
#[derive(Debug)]
pub enum Command {
    /// A client connected (already authenticated + assigned an index).
    ClientOpen { index: u32 },
    /// A client sent an inbound CRDT batch.
    ClientCrdt { index: u32, body: Vec<u8> },
    /// A client disconnected.
    ClientClose { index: u32 },
    /// Stop the game loop and tear the isolate down.
    Shutdown,
}

/// Per-client message plumbing shared between the JS thread and the server.
struct ClientChannel {
    /// CRDT batches received from this client, awaiting the scene's
    /// `client.getMessages()` pull (matches upstream `clientMessages`).
    inbound: VecDeque<Vec<u8>>,
    /// Outbound encoded CRDT bodies the scene pushed via
    /// `client.sendCrdtMessage()`, awaiting flush to the socket.
    outbound: VecDeque<Vec<u8>>,
    /// Whether the scene's observer has seen the `open` event yet.
    open_delivered: bool,
    /// Set when the server has signalled close; the next tick fires the
    /// observer `close` event and reclaims the entity range.
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

/// State the host callbacks mutate. Stored in the isolate's type slot so the
/// extern "C" V8 callbacks can reach it.
struct HostState {
    /// The authoritative CRDT engine — merges every accepted op.
    engine: Arc<Mutex<CrdtEngine>>,
    /// The latest snapshot the scene published via `updateCRDTState`, plus the
    /// engine snapshot as a fallback. Late joiners receive this in `Init`.
    snapshot: Arc<Mutex<Vec<u8>>>,
    /// Entity-range policy the scene declared via `registerScene`.
    config: Arc<Mutex<ServerTransportConfig>>,
    /// True once `registerScene` ran (an observer is registered).
    observer_registered: bool,
    /// Per-client channels keyed by client index.
    clients: std::collections::BTreeMap<u32, ClientChannel>,
    /// The realm name reported by `Runtime.getRealm()`.
    realm_name: String,
    /// Scene hash, reported by `getSceneInformation`.
    scene_hash: String,
    /// Per-client inbound queue cap; beyond it the client is marked for close.
    client_inbound_max: usize,
    /// Per-client outbound queue cap; beyond it the oldest body is dropped.
    client_outbound_max: usize,
}

/// Shared `Arc<Mutex<..>>` handles cloned out of a [`HostState`] borrow.
type SharedHandles = (
    Arc<Mutex<CrdtEngine>>,
    Arc<Mutex<Vec<u8>>>,
    Arc<Mutex<ServerTransportConfig>>,
);

impl HostState {
    /// Borrow the per-isolate `RefCell<HostState>` for the duration of a single
    /// closure. The isolate slot holds a `*const RefCell<HostState>` that lives
    /// for the whole isolate lifetime on this (single) JS thread, so the
    /// reference is valid; the `RefCell` makes any accidental reentrancy a
    /// detected panic-on-borrow instead of undefined behaviour.
    ///
    /// SAFETY: never hold the returned borrow across a V8 call that can
    /// re-enter JS — re-entry would attempt a second `borrow`/`borrow_mut`
    /// while this one is live and panic. All callers below borrow only for a
    /// short, non-reentrant region (snapshot what they need, drop the borrow,
    /// then call into JS).
    fn with<R>(isolate: &mut v8::Isolate, f: impl FnOnce(&RefCell<HostState>) -> R) -> R {
        let ptr = isolate.get_data(0) as *const RefCell<HostState>;
        let cell = unsafe { &*ptr };
        f(cell)
    }

    /// Clone out the shared `Arc<Mutex<..>>` handles under a short borrow, so
    /// the caller can lock the engine/snapshot without holding the `RefCell`
    /// borrow open (which would risk a reentrant-borrow panic if a callback
    /// nested). The mutexes themselves are independent of the `RefCell`.
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

/// State shared with the async server (read by WS tasks for the `Init` frame).
pub struct SharedState {
    pub engine: Arc<Mutex<CrdtEngine>>,
    pub snapshot: Arc<Mutex<Vec<u8>>>,
    pub config: Arc<Mutex<ServerTransportConfig>>,
    /// Outbound per-client queues drained by the WS task. The JS thread pushes;
    /// the server pops. `None` once the client is gone.
    pub outbound: dashmap::DashMap<u32, mpsc::Sender<Vec<u8>>>,
    pub running: AtomicBool,
}

/// Shared between the JS thread and its watchdog. The JS thread publishes a
/// per-tick deadline (monotonic millis since an epoch `Instant`); the watchdog
/// thread terminates execution once `now > deadline` while a tick is in
/// flight. `tick_active` gates the watchdog so it never fires between ticks.
struct Watchdog {
    /// Monotonic millis (since `epoch`) by which the current tick must finish.
    deadline_ms: AtomicU64,
    /// True while a single JS tick is executing (the watchdog only fires then).
    tick_active: AtomicBool,
    /// Set by the watchdog (or heap callback) when it force-terminated the
    /// current guarded region. The JS thread reads + clears it after each
    /// guarded call to decide whether to abort the scene.
    fired: AtomicBool,
    /// Set once the JS thread is shutting down so the watchdog can exit.
    stopped: AtomicBool,
    /// Monotonic epoch the deadline is measured from.
    epoch: std::time::Instant,
    /// Thread-safe terminate handle for the isolate.
    isolate: parking_lot::Mutex<Option<v8::IsolateHandle>>,
}

impl Watchdog {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

/// Handle the rest of the crate holds. Sends [`Command`]s into the JS thread.
pub struct JsRuntimeHandle {
    pub scene_hash: String,
    pub shared: Arc<SharedState>,
    pub tx: mpsc::UnboundedSender<Command>,
    next_index: AtomicU32,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Watchdog shared state, used by `shutdown()` to force-terminate a wedged
    /// isolate so the timed join can succeed.
    watchdog: Arc<Watchdog>,
    /// How long `shutdown()` waits for the JS thread to unwind before giving up.
    shutdown_join: std::time::Duration,
}

impl JsRuntimeHandle {
    pub fn next_client_index(&self) -> u32 {
        self.next_index.fetch_add(1, Ordering::SeqCst)
    }

    /// Stop the scene. Asks the loop to stop, force-terminates any in-flight JS
    /// execution (so an infinite loop in onUpdate can't pin the thread), then
    /// joins with a timeout so a truly wedged thread can never block the caller
    /// (e.g. `/debugging/reload` or `load_or_reload`'s drop) forever.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
        self.shared.running.store(false, Ordering::SeqCst);
        // Force-terminate any in-flight JS execution so the loop can reach its
        // shutdown check even if the scene is in an infinite loop.
        self.watchdog.stopped.store(true, Ordering::SeqCst);
        if let Some(h) = self.watchdog.isolate.lock().as_ref() {
            h.terminate_execution();
        }
        // Timed join: poll the JoinHandle so we never block forever on a wedged
        // thread. If it doesn't exit within the budget we detach it (leaking the
        // thread + isolate is far better than deadlocking the whole server).
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
                // Give up: detach the handle so Drop doesn't re-block. The
                // terminate_execution above should let it die eventually.
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

/// Spawn the JS thread for `source` and return a handle. The thread evaluates
/// the scene, runs `onStart`, then drives the 30 Hz `onUpdate` loop until a
/// `Shutdown` command or a fatal scene error.
pub fn spawn(
    scene_hash: String,
    source: String,
    realm_name: String,
    limits: RuntimeLimits,
    static_crdt: Vec<u8>,
) -> JsRuntimeHandle {
    let engine = Arc::new(Mutex::new(CrdtEngine::with_cap(limits.crdt_max_components)));
    let snapshot = Arc::new(Mutex::new(Vec::new()));

    // Seed the static main.crdt (scene-composer entities) into the engine so a
    // late-joining client's Init frame carries the scene geometry. game.js's
    // own onStart/onUpdate ops then merge on top. Without this a hosted scene
    // serves an empty world (static entities live in main.crdt, not game.js).
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

/// The near-heap-limit callback: terminate the scene's execution and bump the
/// limit slightly so V8 has room to unwind cleanly instead of hard-aborting the
/// whole process on OOM. `data` is a `*const Watchdog`.
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
    // Grant a small headroom (32 MiB) so the termination exception can
    // propagate without V8 calling its fatal OOM handler.
    current_heap_limit + 32 * 1024 * 1024
}

/// Body of the dedicated JS thread.
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

    // HostState lives behind a RefCell so the V8 callbacks (which re-enter from
    // inside JS calls) take short, dynamically-checked borrows instead of
    // aliasing `&mut` references (which would be instant UB). The box is owned
    // by this thread for the whole isolate lifetime; the isolate slot holds a
    // raw pointer to it.
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

    // Create the isolate with a hard heap cap. When the heap nears the limit
    // the near-heap callback terminates execution (and grants headroom) so the
    // scene's allocation storm can't trip V8's fatal process-wide OOM abort.
    let heap_max = limits.js_heap_limit_mb.saturating_mul(1024 * 1024);
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default().heap_limits(0, heap_max));
    isolate.set_data(0, host_ptr);

    // Publish the thread-safe terminate handle to the watchdog + heap callback,
    // then arm both safety nets.
    *watchdog.isolate.lock() = Some(isolate.thread_safe_handle());
    let wd_ptr = Arc::as_ptr(&watchdog) as *mut c_void;
    isolate.add_near_heap_limit_callback(near_heap_limit_cb, wd_ptr);

    // Spawn the per-tick wall-clock watchdog. It only fires while a tick is
    // active (between ticks it idles), terminating execution if the tick blows
    // its budget — catching infinite loops in onStart/onUpdate.
    let wd_thread = {
        let watchdog = Arc::clone(&watchdog);
        std::thread::Builder::new()
            .name(format!("scene-wd-{scene_hash}"))
            .spawn(move || watchdog_loop(watchdog))
            .ok()
    };

    let budget_ms = limits.js_tick_budget_ms.max(1);

    // Helper closures can't borrow `scope`, so we inline arm/disarm at each
    // call site via the small `guarded` macro-like block below.

    v8::scope!(let handle_scope, isolate);
    let context = v8::Context::new(handle_scope, Default::default());
    let scope = &mut v8::ContextScope::new(handle_scope, context);

    // Install the sandbox global surface (module/exports/console/require/
    // registerScene/updateCRDTState/setImmediate + restricted fetch/WS).
    install_globals(scope, context);

    // Evaluate the scene source under the wall-clock guard (a scene whose
    // top-level body is an infinite loop must not pin the thread).
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

    // runStart(): call exports.onStart if present, then drain setImmediate.
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

    // First update always uses dt = 0.0 (matches upstream).
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
        // Drain all pending server commands first. Short borrows only.
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
                            // Inbound backlog: the scene isn't draining this
                            // client's getMessages() fast enough. Drop the
                            // client rather than grow without bound.
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

        // Fire pending observer open/close events into the scene (guarded).
        watchdog.arm(budget_ms);
        deliver_client_events(scope, context);
        watchdog.disarm();
        if watchdog.was_terminated() {
            tracing::error!(scene = %scene_hash, "observer callback exceeded budget / OOM; aborting scene");
            break;
        }

        // Run the scene tick.
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

        // Flush outbound queues to the server (WS senders). Short borrow.
        flush_outbound(&host, &shared);

        // Reap fully-closed clients.
        host.borrow_mut().clients.retain(|_, c| !c.closed);

        if !has_on_update {
            // Static scene: still service commands, but slowly.
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

/// Mark the scene stopped, retire the watchdog, and log. Must run on the JS
/// thread before the isolate (and thus the `HostState` box behind the isolate's
/// data slot) is dropped, so the watchdog's `IsolateHandle` is cleared first.
fn finish(
    shared: &SharedState,
    watchdog: &Watchdog,
    wd_thread: Option<std::thread::JoinHandle<()>>,
    scene_hash: &str,
) {
    shared.running.store(false, Ordering::SeqCst);
    watchdog.stopped.store(true, Ordering::SeqCst);
    // Drop the isolate handle so the watchdog can't terminate a dead isolate.
    *watchdog.isolate.lock() = None;
    if let Some(j) = wd_thread {
        let _ = j.join();
    }
    tracing::info!(scene = %scene_hash, "scene JS loop stopped");
}

impl Watchdog {
    /// Begin guarding a JS region: set the deadline `budget_ms` from now and
    /// mark a tick active so the watchdog thread will fire if it overruns.
    fn arm(&self, budget_ms: u64) {
        self.fired.store(false, Ordering::SeqCst);
        self.deadline_ms
            .store(self.now_ms().saturating_add(budget_ms), Ordering::SeqCst);
        self.tick_active.store(true, Ordering::SeqCst);
    }

    /// End the guarded region.
    fn disarm(&self) {
        self.tick_active.store(false, Ordering::SeqCst);
    }

    /// Whether the watchdog or heap callback force-terminated the just-ended
    /// guarded region. Reads + clears the `fired` flag and clears the isolate's
    /// pending termination state so the isolate can run cleanup. Callers abort
    /// the scene on `true` (the terminal path).
    fn was_terminated(&self) -> bool {
        let fired = self.fired.swap(false, Ordering::SeqCst);
        if fired {
            if let Some(h) = self.isolate.lock().as_ref() {
                // Clear the pending uncatchable-termination so the isolate
                // doesn't keep terminating during teardown.
                h.cancel_terminate_execution();
            }
        }
        fired
    }
}

/// Watchdog thread: while a tick is active, terminate execution once its
/// deadline passes. Polls at a fine granularity; exits when `stopped`.
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
                // Avoid re-terminating in a tight loop; wait for disarm.
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

/// Push each client's queued outbound bytes to its WS sender. The queued bytes
/// are raw CRDT bodies; we wrap each into a `Crdt` WS frame here so the WS task
/// can forward verbatim. Uses a short `borrow_mut` (not held across any V8
/// call, so no reentrancy).
fn flush_outbound(host: &RefCell<HostState>, shared: &SharedState) {
    let mut host = host.borrow_mut();
    for (index, ch) in host.clients.iter_mut() {
        if ch.outbound.is_empty() {
            continue;
        }
        if let Some(sender) = shared.outbound.get(index) {
            while let Some(body) = ch.outbound.pop_front() {
                // Bounded sender: if the socket side is full (slow client), the
                // body is dropped rather than blocking the JS thread.
                let _ = sender.try_send(crate::runtime::frame_crdt(&body));
            }
        } else {
            ch.outbound.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// V8 host callbacks
// ---------------------------------------------------------------------------

/// Read a JS function-argument `Uint8Array` (or array-like) into Vec<u8>.
fn read_uint8array(_scope: &mut v8::PinScope, val: v8::Local<v8::Value>) -> Option<Vec<u8>> {
    let view = v8::Local::<v8::ArrayBufferView>::try_from(val).ok()?;
    let len = view.byte_length();
    let mut out = vec![0u8; len];
    view.copy_contents(&mut out);
    Some(out)
}

/// Build a JS `Uint8Array` from bytes.
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

/// Set `obj[key] = value`.
fn set_prop(
    scope: &mut v8::PinScope,
    obj: v8::Local<v8::Object>,
    key: &str,
    value: v8::Local<v8::Value>,
) {
    let k = str(scope, key).into();
    obj.set(scope, k, value);
}

/// Attach a native function as `obj[name]`.
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

/// `EngineApi.crdtSendToRenderer({ data: Uint8Array }) -> { data: [Uint8Array] }`
///
/// The scene's renderer transport calls this every tick with the batch of CRDT
/// ops it produced. We merge them into the authoritative engine and return any
/// messages the scene should receive back (currently none from the renderer
/// path — client merges are surfaced through the per-client `getMessages`).
fn op_crdt_send_to_renderer(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let (engine, snapshot, _cfg) = HostState::shared_handles(scope);
    // arg0 = { data: Uint8Array }
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
    // Refresh the snapshot from the engine after the scene's own writes.
    {
        let eng = engine.lock();
        *snapshot.lock() = eng.snapshot();
    }

    // Return { data: [] } — no messages flow back to the scene from the
    // renderer transport on the server (clients are serviced separately).
    let result = v8::Object::new(scope);
    let empty = v8::Array::new(scope, 0);
    set_prop(scope, result, "data", empty.into());
    rv.set(result.into());
}

/// `EngineApi.crdtGetState() -> { hasEntities: bool, data: [Uint8Array] }`
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

/// `EngineApi.isServer() -> { isServer: true }`
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

/// `EngineApi.sendBatch() -> { events: [] }`
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

/// `Runtime.getRealm() -> { realmInfo: { isPreview: false, realmName } }`
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

/// `Runtime.getSceneInformation() -> { urn, baseUrl, content:[], metadataJson }`
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

/// `Runtime.readFile({ fileName }) -> { content: Uint8Array, hash }`
/// The server has no scene file system here, so it returns empty content
/// (upstream's server runtime likewise has no file-serving path).
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

/// Stub for ASYNC host methods: returns `Promise.resolve({})`. SDK7 awaits /
/// `.then()`s these (e.g. RestrictedActions.movePlayerTo), so a plain object
/// fails with "n(...).then is not a function".
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

/// `registerScene(serverConfig, observer)` — capture the entity-range policy
/// and the client observer. The observer is stored on the global as
/// `__observer` so the loop can call it for open/close events.
fn op_register_scene(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    // serverConfig: { reservedLocalEntities, networkEntitiesLimit:{serverLimit, clientLimit} }
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

    // Stash the observer function on the global as `__observer`.
    let observer = args.get(1);
    if observer.is_function() {
        let ctx = scope.get_current_context();
        let global = ctx.global(scope);
        let key = str(scope, "__observer").into();
        global.set(scope, key, observer);
    }
}

/// `updateCRDTState(value: Uint8Array)` — the scene publishes its full snapshot.
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

/// `console.log` / `console.error` — route to tracing.
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

/// `setImmediate(fn)` — push onto the `__setImmediate` array on the global.
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

/// `require('~system/<mod>')` -> the host module object, or throws.
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

/// `fetch`/`WebSocket` are disabled on the server (createWsFetchRuntime).
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

/// Construct a `~system/<module>` object with its functions.
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
        // Server-side there is no local player to move/emote, so every
        // RestrictedActions method is a no-op resolving to {} — without this the
        // scene's `import '~system/RestrictedActions'` throws "Unknown module" and
        // the whole game.js eval fails before onStart (hit by olavra.dcl.eth).
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
        "EnvironmentApi" | "EnvironmentAPI" => { /* empty object */ }
        // Any other standard SDK7 host module (CommsApi, Players,
        // PortableExperiences, PlayerIdentityData, …) resolves to an empty
        // object on this headless host instead of crashing the scene import.
        // Methods the scene then calls return undefined, which SDK7 call-sites
        // tolerate; a hard "Unknown module" throw aborts the whole scene.
        _ => {}
    }
    Some(obj)
}

/// Install the sandbox global surface onto the context's global object.
fn install_globals(scope: &mut v8::PinScope, context: v8::Local<v8::Context>) {
    let global = context.global(scope);

    // module = { exports: {} }; expose module + exports.
    let module = v8::Object::new(scope);
    let exports = v8::Object::new(scope);
    set_prop(scope, module, "exports", exports.into());
    set_prop(scope, global, "module", module.into());
    set_prop(scope, global, "exports", exports.into());

    // console
    let console = v8::Object::new(scope);
    for m in ["log", "info", "debug", "trace", "warning", "error"] {
        set_fn(scope, console, m, op_console_log);
    }
    set_prop(scope, global, "console", console.into());

    // require / setImmediate / registerScene / updateCRDTState
    set_fn(scope, global, "require", op_require);
    set_fn(scope, global, "setImmediate", op_set_immediate);
    set_fn(scope, global, "registerScene", op_register_scene);
    set_fn(scope, global, "updateCRDTState", op_update_crdt_state);

    // restricted fetch + WebSocket
    set_fn(scope, global, "fetch", op_restricted);
    set_fn(scope, global, "WebSocket", op_restricted);

    // globalThis self-reference (some bundles expect `self`).
    let g: v8::Local<v8::Value> = global.into();
    set_prop(scope, global, "self", g);
    set_prop(scope, global, "global", g);
}

/// Compile + run the scene source against the current context.
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

/// Call `exports.<name>(arg?)`, awaiting nothing (the server treats the
/// returned promise as fire-and-forget, matching the JS `await` being a
/// microtask the loop pumps via setImmediate drain + message loop).
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
        _ => return Ok(()), // absent export is a no-op
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
    // Pump microtasks so any awaited promises inside onStart/onUpdate progress.
    tc.perform_microtask_checkpoint();
    Ok(())
}

/// Drain `globalThis.__setImmediate`, invoking each queued fn (sdk7-runtime's
/// runSetImmediate).
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
    // Reset the array.
    let fresh = v8::Array::new(scope, 0);
    global.set(scope, key, fresh.into());
}

/// Build a per-client JS object `{ sendCrdtMessage(bytes), getMessages() }` and
/// fire the observer's open event; also fire close events for closing clients.
fn deliver_client_events(scope: &mut v8::PinScope, context: v8::Local<v8::Context>) {
    // Snapshot everything we need under a short borrow, then drop it — the
    // observer call below re-enters JS, which synchronously calls
    // op_client_send / op_client_get_messages / crdtSendToRenderer, each of
    // which re-derives the host state. Holding a borrow across that call is the
    // UB the original code had (two live `&mut HostState`); with the `RefCell`
    // a stray borrow would now panic, so we make sure none is held.
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
        // Mark open delivered BEFORE the call (short borrow, dropped here).
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
        // No host borrow held across this re-entrant call.
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
            // No host borrow held across this re-entrant call.
            v8::tc_scope!(let tc, scope);
            let _ = observer.call(tc, recv, &[event.into()]);
        }
        // Reclaim the client's network entity range and broadcast deletes. The
        // engine/snapshot are independent Arc<Mutex>; clone them out under a
        // short borrow, then lock without holding the RefCell borrow.
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

/// Build the `{ sendCrdtMessage, getMessages }` client object. The client index
/// is captured via an integer baked into the function's data slot (we encode it
/// in the function's bound name through an external-backed closure object).
fn build_client_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: u32,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    // Stash the client index on the object so the callbacks can read it via
    // `this.__index` (callbacks receive `this` == the client object).
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

/// `client.sendCrdtMessage(message)` — queue an outbound CRDT body for this
/// client and merge it into the authoritative engine (the scene is the writer).
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
    // Merge the scene's authoritative output so the snapshot/late-joiner state
    // reflects it. Engine/snapshot are independent Arc<Mutex>; lock them
    // without holding the host RefCell borrow.
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
            // Bound the per-client outbound queue: drop the oldest body if the
            // client isn't being drained (slow/idle reader) instead of growing
            // without bound.
            if ch.outbound.len() >= cap {
                ch.outbound.pop_front();
            }
            // The body is wrapped into a Crdt frame by the server side.
            ch.outbound.push_back(bytes);
        }
    });
}

/// `client.getMessages() -> Uint8Array[]` — drain this client's inbound queue.
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

    // Wait until `pred` is true or a timeout elapses (the JS loop runs at 30Hz
    // on its own thread, so we poll the shared state).
    fn wait_for<F: Fn() -> bool>(pred: F) -> bool {
        for _ in 0..200 {
            if pred() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        pred()
    }

    /// A scene whose onStart pushes one PUT to the renderer transport. Asserts
    /// the engine merged it and the snapshot reflects it — proves eval + onStart
    /// + the EngineApi.crdtSendToRenderer host op + the CRDT engine all work.
    #[test]
    fn scene_onstart_writes_to_engine() {
        // PUT entity 600, comp 1, ts 1, data [42] — hand-encoded the same way
        // the SDK's ByteBuffer would (little-endian).
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

    /// A scene that, in onUpdate, drains each client's inbound messages and
    /// echoes a derived PUT back via client.sendCrdtMessage. Proves the full
    /// registerScene observer -> per-client getMessages/sendCrdtMessage loop +
    /// the outbound framing path.
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

        // Open a client and register its outbound sink.
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
        let index = handle.next_client_index();
        handle.shared.outbound.insert(index, tx);
        handle.tx.send(Command::ClientOpen { index }).unwrap();

        // Give the loop a tick to deliver the observer 'open' event.
        std::thread::sleep(Duration::from_millis(60));

        // Send a CRDT batch from the client.
        let body = encode_batch(&[put(1100, 1, 7, &[1, 2, 3])]);
        handle
            .tx
            .send(Command::ClientCrdt {
                index,
                body: body.clone(),
            })
            .unwrap();

        // The scene should echo it back as a Crdt frame.
        let frame = wait_for_recv(&mut rx);
        assert!(frame.is_some(), "expected an echoed Crdt frame");
        let frame = frame.unwrap();
        // First byte is the WS Crdt message type (3); the rest is the CRDT body.
        assert_eq!(frame[0], crate::protocol::MessageType::Crdt as u8);
        assert_eq!(decode_batch(&frame[1..]), vec![put(1100, 1, 7, &[1, 2, 3])]);
    }

    /// On client close the runtime reclaims the client's entity range: its
    /// network entities are tombstoned out of the authoritative engine.
    #[test]
    fn client_close_reclaims_range() {
        // Static scene (no onUpdate); just registers an observer so open/close
        // events flow.
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
        let index = handle.next_client_index(); // 0 -> range [1024,1536)
        handle.shared.outbound.insert(index, tx);
        handle.tx.send(Command::ClientOpen { index }).unwrap();
        std::thread::sleep(Duration::from_millis(60));

        // Client writes an entity inside its range.
        let body = encode_batch(&[put(1100, 1, 1, &[1])]);
        handle.tx.send(Command::ClientCrdt { index, body }).unwrap();
        // Statically the scene doesn't merge client msgs, so write directly to
        // the engine to model the merged state, then close and assert reclaim.
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

    /// A scene whose onUpdate never returns (infinite loop) must be terminated
    /// by the wall-clock watchdog, the loop must stop, and shutdown() must NOT
    /// hang (timed join). This exercises fix #1 (CPU/wall-clock guard +
    /// non-blocking shutdown).
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

        // The loop should stop on its own once the watchdog terminates the tick.
        let stopped = wait_for(|| !handle.shared.running.load(Ordering::SeqCst));
        assert!(
            stopped,
            "watchdog should have terminated the infinite onUpdate"
        );

        // shutdown() must return promptly even though the scene was wedged.
        let start = std::time::Instant::now();
        handle.shutdown();
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "shutdown() must not block on a wedged scene"
        );
    }

    /// The CRDT component cap rejects new cells beyond the limit (fix #3).
    #[test]
    fn crdt_cap_rejects_new_cells() {
        use crate::crdt::{ApplyResult, CrdtEngine};
        let mut e = CrdtEngine::with_cap(2);
        assert_eq!(e.apply(&put(1, 1, 1, b"a")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 2, 1, b"b")), ApplyResult::Applied);
        // Third distinct cell is over the cap -> rejected.
        assert_eq!(e.apply(&put(1, 3, 1, b"c")), ApplyResult::Ignored);
        // Updating an existing cell is still allowed.
        assert_eq!(e.apply(&put(1, 1, 2, b"a2")), ApplyResult::Applied);
        assert_eq!(e.component_count(), 2);
    }
}
