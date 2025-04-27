use std::collections::HashMap;
use std::any::{TypeId, Any};

// Type alias for our listeners. They are boxed closures that can be mutated
// and accept a reference to *any* type that has been boxed.
type Listener = Box<dyn FnMut(&dyn Any)>;

// The central event manager
pub struct EventManager {
    // Stores listeners keyed by the TypeId of the event they listen to.
    listeners: HashMap<TypeId, Vec<Listener>>,
}

impl EventManager {
    pub fn new() -> Self {
        EventManager {
            listeners: HashMap::new(),
        }
    }

    /// Subscribes a listener closure to a specific event type `E`.
    /// The listener must be 'static (cannot hold non-static references).
    pub fn subscribe<E: Any + 'static>(&mut self, mut listener: impl FnMut(&E) + 'static) {
        let type_id = TypeId::of::<E>();
        let listeners = self.listeners.entry(type_id).or_insert_with(Vec::new);

        // Wrap the specific listener `FnMut(&E)` into a generic `FnMut(&dyn Any)`.
        // This boxed listener will attempt to downcast the received `&dyn Any`
        // back to the specific type `&E` it knows how to handle.
        let boxed_listener = Box::new(move |event: &dyn Any| {
            if let Some(specific_event) = event.downcast_ref::<E>() {
                listener(specific_event);
            }
        });

        listeners.push(boxed_listener);
    }

    /// Dispatches an event to all registered listeners for that event type `E`.
    /// The event itself must be 'static.
    pub fn dispatch<E: Any + 'static>(&mut self, event: &E) {
        let type_id = TypeId::of::<E>();
        // Get the list of listeners for this event type, if any.
        if let Some(listeners) = self.listeners.get_mut(&type_id) {
            // Iterate through the listeners and call each one.
            // The listener closure itself handles the downcasting.
            for listener in listeners {
                listener(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[derive(Debug, Clone)]
    struct PlayerJumped {
        player_id: u32,
        height: f32,
    }

    #[derive(Debug, Clone)]
    struct EnemySpawned {
        enemy_type: String,
        position: (f32, f32),
    }

    #[test]
    fn simple_test_event_manager() {
        let mut event_manager = EventManager::new();

        let (tx_jump, rx_jump) = mpsc::channel::<(u32, f32)>();

        let tx_jump1 = tx_jump.clone();
        event_manager.subscribe(move |event: &PlayerJumped| {
            let _ = tx_jump1.send((event.player_id, event.height));
        });

        let jump_event1 = PlayerJumped { player_id: 1, height: 10.5 };
        event_manager.dispatch(&jump_event1);

        let timeout = Duration::from_millis(100);

        let received_jump = rx_jump.recv_timeout(timeout).expect("Listener 1 for jump 1 timed out");
        assert_eq!(received_jump, (1, 10.5));

        assert!(rx_jump.try_recv().is_err(), "Should be no more jump events");
    }

    #[test]
    fn test_event_manager() {
        let mut event_manager = EventManager::new();

        let (tx_jump, rx_jump) = mpsc::channel::<(u32, f32)>();
        let (tx_spawn, rx_spawn) = mpsc::channel::<(String, (f32, f32))>();

        let tx_jump1 = tx_jump.clone();
        event_manager.subscribe(move |event: &PlayerJumped| {
            let _ = tx_jump1.send((event.player_id, event.height));
        });

        let tx_jump2 = tx_jump.clone();
        event_manager.subscribe(move |event: &PlayerJumped| {
            let _ = tx_jump2.send((event.player_id, 0.0));
        });

        let tx_spawn1 = tx_spawn.clone();
        event_manager.subscribe(move |event: &EnemySpawned| {
            let _ = tx_spawn1.send((event.enemy_type.clone(), event.position));
        });

        let jump_event1 = PlayerJumped { player_id: 1, height: 10.5 };
        event_manager.dispatch(&jump_event1);

        let spawn_event = EnemySpawned {
            enemy_type: "Goblin".to_string(),
            position: (10.0, 5.0),
        };
        event_manager.dispatch(&spawn_event);

        let jump_event2 = PlayerJumped { player_id: 2, height: 8.0 };
        event_manager.dispatch(&jump_event2);

        let timeout = Duration::from_millis(100);

        let mut received_jumps = Vec::new();
        received_jumps.push(rx_jump.recv_timeout(timeout).expect("Listener 1 for jump 1 timed out"));
        received_jumps.push(rx_jump.recv_timeout(timeout).expect("Listener 2 for jump 1 timed out"));
        received_jumps.push(rx_jump.recv_timeout(timeout).expect("Listener 1 for jump 2 timed out"));
        received_jumps.push(rx_jump.recv_timeout(timeout).expect("Listener 2 for jump 2 timed out"));

        received_jumps.sort_by_key(|k| (k.0, k.1 as u32));

        assert_eq!(
            received_jumps,
            vec![(1, 0.0), (1, 10.5), (2, 0.0), (2, 8.0)],
            "Mismatch in received PlayerJumped events"
        );

        let received_spawn = rx_spawn.recv_timeout(timeout).expect("Listener for spawn 1 timed out");
        assert_eq!(
            received_spawn,
            ("Goblin".to_string(), (10.0, 5.0)),
            "Mismatch in received EnemySpawned event"
        );

        assert!(rx_jump.try_recv().is_err(), "Should be no more jump events");
        assert!(rx_spawn.try_recv().is_err(), "Should be no more spawn events");
    }
}