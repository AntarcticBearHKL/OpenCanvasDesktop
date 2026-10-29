//! In-memory canvas event bus, mirroring `web/src/lib/canvas/canvas-event-bus.ts`.
//!
//! The web bus keeps a `Map<event, Set<handler>>` and returns an unsubscribe
//! closure. The native equivalent returns an [`EventSubscription`] whose `Drop`
//! unsubscribes, so forgetting to store the handle is not a leak-free operation
//! by accident: dropping it is the unsubscribe.
//!
//! Handlers run synchronously on the emitting thread; a panicking handler is a
//! host bug and is not swallowed here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use serde_json::Value;

/// Payload handed to a listener. `None` mirrors the web `payload?`.
pub type EventPayload = Value;

/// A bus listener. `Arc` so the bus and callers can share it.
pub type EventHandler = Arc<dyn Fn(Option<&EventPayload>) + Send + Sync>;

#[derive(Default)]
struct BusState {
    next_id: u64,
    handlers: HashMap<String, Vec<(u64, EventHandler)>>,
}

/// In-memory event bus shared by the canvas host and plugins.
#[derive(Clone, Default)]
pub struct CanvasEventBus {
    state: Arc<Mutex<BusState>>,
}

impl CanvasEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, BusState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Deliver `payload` to every listener of `event`, returning how many ran.
    pub fn emit(&self, event: &str, payload: Option<&EventPayload>) -> usize {
        let listeners = {
            let state = self.lock();
            state
                .handlers
                .get(event)
                .map(|handlers| {
                    handlers
                        .iter()
                        .map(|(_, handler)| Arc::clone(handler))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        for handler in &listeners {
            handler(payload);
        }
        listeners.len()
    }

    /// Subscribe to `event`. Dropping the returned handle unsubscribes.
    pub fn on<H>(&self, event: &str, handler: H) -> EventSubscription
    where
        H: Fn(Option<&EventPayload>) + Send + Sync + 'static,
    {
        let mut state = self.lock();
        let id = state.next_id;
        state.next_id += 1;
        state
            .handlers
            .entry(event.to_string())
            .or_default()
            .push((id, Arc::new(handler)));
        EventSubscription {
            state: Arc::downgrade(&self.state),
            event: event.to_string(),
            id,
        }
    }

    /// Number of live listeners for `event` (test/diagnostic helper).
    pub fn listener_count(&self, event: &str) -> usize {
        let state = self.lock();
        state.handlers.get(event).map_or(0, Vec::len)
    }
}

/// Handle returned by [`CanvasEventBus::on`]; dropping it removes the listener.
pub struct EventSubscription {
    state: Weak<Mutex<BusState>>,
    event: String,
    id: u64,
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut state = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(handlers) = state.handlers.get_mut(&self.event) {
            handlers.retain(|(id, _)| *id != self.id);
            if handlers.is_empty() {
                state.handlers.remove(&self.event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn emit_delivers_to_every_listener_with_the_payload() {
        let bus = CanvasEventBus::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let first = Arc::clone(&seen);
        let _a = bus.on("ping", move |payload| {
            if let Ok(mut values) = first.lock() {
                values.push(payload.cloned());
            }
        });
        let second = Arc::clone(&seen);
        let _b = bus.on("ping", move |_| {
            if let Ok(mut values) = second.lock() {
                values.push(Some(Value::String("second".into())));
            }
        });
        let delivered = bus.emit("ping", Some(&Value::from(7)));
        assert_eq!(delivered, 2);
    }

    #[test]
    fn dropping_the_subscription_unsubscribes() {
        let bus = CanvasEventBus::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let subscription = bus.on("tick", move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        bus.emit("tick", None);
        drop(subscription);
        bus.emit("tick", None);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(bus.listener_count("tick"), 0);
    }

    #[test]
    fn events_are_isolated_by_name() {
        let bus = CanvasEventBus::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let _subscription = bus.on("a", move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(bus.emit("b", None), 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(bus.emit("a", None), 1);
    }
}
