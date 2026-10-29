use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use open_canvas_plugins::{CanvasEventBus, MemoryPluginStore, PluginStorage};
use serde_json::json;

#[test]
fn listeners_run_in_registration_order() {
    let bus = CanvasEventBus::new();
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let first = Arc::clone(&order);
    let _a = bus.on("go", move |_| {
        if let Ok(mut log) = first.lock() {
            log.push(1);
        }
    });
    let second = Arc::clone(&order);
    let _b = bus.on("go", move |_| {
        if let Ok(mut log) = second.lock() {
            log.push(2);
        }
    });
    assert_eq!(bus.emit("go", None), 2);
    assert_eq!(*order.lock().expect("order lock"), vec![1, 2]);
}

#[test]
fn payload_is_optional_and_passed_through() {
    let bus = CanvasEventBus::new();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let _subscription = bus.on("data", move |payload| {
        if let Ok(mut log) = sink.lock() {
            log.push(payload.cloned());
        }
    });
    bus.emit("data", None);
    bus.emit("data", Some(&json!({ "n": 1 })));
    let log = seen.lock().expect("sink lock");
    assert_eq!(log.len(), 2);
    assert_eq!(log[0], None);
    assert_eq!(log[1], Some(json!({ "n": 1 })));
}

#[test]
fn dropping_one_of_two_subscriptions_keeps_the_other() {
    let bus = CanvasEventBus::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let first = Arc::clone(&calls);
    let subscription = bus.on("tick", move |_| {
        first.fetch_add(1, Ordering::SeqCst);
    });
    let second = Arc::clone(&calls);
    let _kept = bus.on("tick", move |_| {
        second.fetch_add(1, Ordering::SeqCst);
    });
    drop(subscription);
    assert_eq!(bus.listener_count("tick"), 1);
    bus.emit("tick", None);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn cloned_bus_shares_listeners() {
    let bus = CanvasEventBus::new();
    let clone = bus.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let _subscription = clone.on("ping", move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(bus.emit("ping", None), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn namespaced_storage_is_isolated_per_plugin() {
    let store = MemoryPluginStore::new();
    let alpha = store.storage("alpha");
    let beta = store.storage("beta");
    assert_eq!(alpha.namespace(), "alpha");
    alpha.set("k", json!(1)).expect("set alpha");
    beta.set("k", json!(2)).expect("set beta");
    assert_eq!(alpha.get("k").expect("get alpha"), Some(json!(1)));
    assert_eq!(beta.get("k").expect("get beta"), Some(json!(2)));
    alpha.remove("k").expect("remove alpha");
    assert_eq!(alpha.get("k").expect("get alpha"), None);
    assert_eq!(beta.get("k").expect("get beta"), Some(json!(2)));
}
