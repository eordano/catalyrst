use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::crdt::{decode_batch, CrdtEngine};
use crate::runtime::{RuntimeLimits, ServerTransportConfig};

use super::fetch;
use super::scene_thread::run_scene_thread;

static V8_INIT: std::sync::Once = std::sync::Once::new();

pub(super) fn ensure_v8_initialized() {
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

pub struct SharedState {
    pub engine: Arc<Mutex<CrdtEngine>>,
    pub snapshot: Arc<Mutex<Vec<u8>>>,
    pub config: Arc<Mutex<ServerTransportConfig>>,

    pub outbound: dashmap::DashMap<u32, mpsc::Sender<Vec<u8>>>,
    pub running: AtomicBool,
}

pub(super) struct Watchdog {
    deadline_ms: AtomicU64,

    tick_active: AtomicBool,

    fired: AtomicBool,

    stopped: AtomicBool,

    epoch: std::time::Instant,

    pub(super) isolate: parking_lot::Mutex<Option<v8::IsolateHandle>>,
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
    storage: Option<fetch::StorageCtx>,
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

    // The fetch worker thread is detached on purpose: it exits on its own once
    // the job sender inside the scene thread drops, and joining it here would
    // block shutdown on in-flight requests.
    let fetch_wiring = storage.map(|ctx| {
        let (jobs_tx, jobs_rx) = mpsc::unbounded_channel();
        let (results_tx, results_rx) = std::sync::mpsc::channel();
        drop(fetch::spawn_fetch_worker(
            ctx.clone(),
            limits,
            jobs_rx,
            results_tx,
        ));
        fetch::FetchWiring {
            ctx,
            tx: jobs_tx,
            results: results_rx,
        }
    });

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
                fetch_wiring,
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

pub(super) extern "C" fn near_heap_limit_cb(
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

pub(super) fn finish(
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
    pub(super) fn arm(&self, budget_ms: u64) {
        self.fired.store(false, Ordering::SeqCst);
        self.deadline_ms
            .store(self.now_ms().saturating_add(budget_ms), Ordering::SeqCst);
        self.tick_active.store(true, Ordering::SeqCst);
    }

    pub(super) fn disarm(&self) {
        self.tick_active.store(false, Ordering::SeqCst);
    }

    pub(super) fn was_terminated(&self) -> bool {
        let fired = self.fired.swap(false, Ordering::SeqCst);
        if fired {
            if let Some(h) = self.isolate.lock().as_ref() {
                h.cancel_terminate_execution();
            }
        }
        fired
    }
}

pub(super) fn watchdog_loop(watchdog: Arc<Watchdog>) {
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
            None,
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
    fn restricted_actions_stubs_resolve() {
        let batch = encode_batch(&[put(601, 1, 1, &[7])]);
        let js = format!(
            r#"
            var EngineApi = require('~system/EngineApi');
            var RestrictedActions = require('~system/RestrictedActions');
            module.exports.onStart = async function () {{
              await RestrictedActions.triggerEmote({{ predefinedEmote: 'wave' }});
              await RestrictedActions.stopEmote({{}});
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
            "restricted-actions-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
            None,
        );
        let ok = wait_for(|| {
            let eng = handle.shared.engine.lock();
            eng.component_count() == 1
        });
        assert!(
            ok,
            "restricted-action stubs (incl. stopEmote) must resolve so onStart reaches the write"
        );
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
            None,
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
            None,
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
            None,
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
    fn top_level_var_does_not_clobber_globals() {
        let batch = encode_batch(&[put(700, 1, 1, &[7])]);
        let js = format!(
            r#"
            var EngineApi = require('~system/EngineApi');
            var DEBUG_NETWORK_MESSAGES = () => globalThis.DEBUG_NETWORK_MESSAGES ?? false;
            globalThis.DEBUG_NETWORK_MESSAGES = true;
            module.exports.onStart = async function () {{
              if (DEBUG_NETWORK_MESSAGES() !== true) {{ throw new Error('global clobbered'); }}
              await EngineApi.crdtSendToRenderer({{ data: new Uint8Array({batch:?}) }});
            }};
            "#
        );
        let handle = spawn(
            "wrapper-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
            None,
        );
        let ok = wait_for(|| handle.shared.engine.lock().component_count() == 1);
        assert!(
            ok,
            "top-level var collided with a global; the CJS wrapper must isolate it"
        );
        let snap = handle.shared.snapshot.lock().clone();
        assert_eq!(decode_batch(&snap), vec![put(700, 1, 1, &[7])]);
    }

    #[test]
    fn set_immediate_queued_during_drain_runs_next_drain() {
        let first = encode_batch(&[put(600, 1, 1, &[1])]);
        let second = encode_batch(&[put(601, 1, 1, &[2])]);
        let js = format!(
            r#"
            var EngineApi = require('~system/EngineApi');
            setImmediate(function () {{
              EngineApi.crdtSendToRenderer({{ data: new Uint8Array({first:?}) }});
              setImmediate(function () {{
                EngineApi.crdtSendToRenderer({{ data: new Uint8Array({second:?}) }});
              }});
            }});
            module.exports.onUpdate = function (dt) {{}};
            "#
        );
        let handle = spawn(
            "requeue-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
            None,
        );
        let ok = wait_for(|| handle.shared.engine.lock().component_count() == 2);
        assert!(
            ok,
            "a setImmediate queued during the drain must run on the next drain, not never"
        );
    }

    #[test]
    fn onupdate_throw_skips_frame_and_scene_continues() {
        let batch = encode_batch(&[put(800, 1, 1, &[8])]);
        let js = format!(
            r#"
            var EngineApi = require('~system/EngineApi');
            var ticks = 0;
            module.exports.onUpdate = function (dt) {{
              ticks++;
              if (ticks === 1) {{ throw new Error('frame 1 fails'); }}
              if (ticks === 2) {{ EngineApi.crdtSendToRenderer({{ data: new Uint8Array({batch:?}) }}); }}
            }};
            "#
        );
        let handle = spawn(
            "throwy-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
            None,
        );
        let ok = wait_for(|| handle.shared.engine.lock().component_count() == 1);
        assert!(
            ok,
            "an onUpdate throw must skip the frame, not kill the loop"
        );
        assert!(handle.shared.running.load(Ordering::SeqCst));
    }

    #[test]
    fn onupdate_consecutive_throws_tear_scene_down() {
        let js = r#"
            module.exports.onUpdate = function (dt) { throw new Error('always'); };
        "#
        .to_string();
        let limits = RuntimeLimits {
            js_update_failure_cap: 3,
            ..RuntimeLimits::default()
        };
        let handle = spawn(
            "doomed-scene".into(),
            js,
            "dcl-test".into(),
            limits,
            Vec::new(),
            None,
        );
        let stopped = wait_for(|| !handle.shared.running.load(Ordering::SeqCst));
        assert!(
            stopped,
            "hitting the consecutive-failure cap must tear the scene down"
        );
    }

    #[test]
    fn out_of_range_client_ops_never_reach_the_scene() {
        let js = r#"
            var clients = {};
            registerScene(
              { reservedLocalEntities: 512, networkEntitiesLimit: { serverLimit: 512, clientLimit: 512 } },
              function (ev) {
                if (ev.type === 'open') { clients[ev.clientId] = ev.client; }
                if (ev.type === 'close') { delete clients[ev.clientId]; }
              }
            );
            module.exports.onUpdate = function (dt) {
              for (var id in clients) {
                var msgs = clients[id].getMessages();
                for (var i = 0; i < msgs.length; i++) {
                  clients[id].sendCrdtMessage(msgs[i]);
                }
              }
            };
        "#
        .to_string();
        let handle = spawn(
            "range-scene".into(),
            js,
            "dcl-test".into(),
            RuntimeLimits::default(),
            Vec::new(),
            None,
        );

        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
        let index = handle.next_client_index();
        handle.shared.outbound.insert(index, tx);
        handle.tx.send(Command::ClientOpen { index }).unwrap();
        std::thread::sleep(Duration::from_millis(60));

        let out_of_range = encode_batch(&[
            put(5000, 1, 7, &[9]),
            CrdtMessage::DeleteEntity {
                entity: 4_000_000_000,
            },
        ]);
        handle
            .tx
            .send(Command::ClientCrdt {
                index,
                body: out_of_range,
            })
            .unwrap();
        let in_range = encode_batch(&[put(1100, 1, 7, &[1])]);
        handle
            .tx
            .send(Command::ClientCrdt {
                index,
                body: in_range,
            })
            .unwrap();

        let frame = wait_for_recv(&mut rx).expect("in-range op must still be echoed");
        assert_eq!(decode_batch(&frame[1..]), vec![put(1100, 1, 7, &[1])]);

        let snap = handle.shared.snapshot.lock().clone();
        assert_eq!(decode_batch(&snap), vec![put(1100, 1, 7, &[1])]);
        assert_eq!(handle.shared.engine.lock().deleted_count(), 0);
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
