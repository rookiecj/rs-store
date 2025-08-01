use crate::StoreImpl;
use std::ops::Deref;
use std::sync::Arc;

/// DroppableStore is a wrapper around a Store that will automatically stop the store when dropped.
/// It doesn't care about Arc reference count, this is useful when you want to ensure
/// that the store is stopped when it goes out of scope.
pub struct DroppableStore<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    inner: Arc<StoreImpl<State, Action>>,
}

impl<State, Action> DroppableStore<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    pub fn new(store: Arc<StoreImpl<State, Action>>) -> Self {
        Self { inner: store }
    }
}

impl<State, Action> Drop for DroppableStore<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn drop(&mut self) {
        let _ = self.inner.stop();
        #[cfg(feature = "store-log")]
        eprintln!("store: '{}' Store dropped", self.inner.name);
    }
}

impl<State, Action> Deref for DroppableStore<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    type Target = Arc<StoreImpl<State, Action>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use crate::{DispatchOp, Dispatcher, DroppableStore, FnReducer, StoreBuilder};
    use std::time::Duration;

    #[test]
    fn test_store_cleanup_on_drop() {
        // Create a custom reducer that we can track
        let reducer = move |state: &i32, action: &i32| DispatchOp::Dispatch(state + action, None);

        // Create store in a new scope so it will be dropped
        {
            let store = StoreBuilder::new(0)
                .with_reducer(Box::new(FnReducer::from(reducer)))
                .build()
                .expect("Failed to build store");

            // Dispatch a normal action
            store.dispatch(1).expect("Failed to dispatch action");

            // Wait a bit to ensure action is processed
            std::thread::sleep(Duration::from_millis(50));
            assert_eq!(store.get_state(), 1);

            // Store will be dropped here
        }
    }

    #[test]
    fn test_multiple_store_instances() {
        // Create and drop multiple stores to ensure no resource leaks
        let stores: Vec<_> = (0..3)
            .map(|_| {
                let store = StoreBuilder::new(0)
                    .with_reducer(Box::new(FnReducer::from(
                        move |state: &i32, action: &i32| DispatchOp::Dispatch(state + action, None),
                    )))
                    .build()
                    .expect("Failed to build store");

                store.dispatch(1).expect("Failed to dispatch action");
                store
            })
            .collect();

        // Wait a bit to ensure actions are processed
        std::thread::sleep(Duration::from_millis(50));

        {
            let states: Vec<_> = stores.iter().map(|store| store.get_state()).collect();
            assert_eq!(states, vec![1, 1, 1]);

            // Stores will be dropped here
        }
    }

    #[test]
    fn test_store_cleanup_with_pending_actions() {
        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(FnReducer::from(
                move |state: &i32, action: &i32| {
                    std::thread::sleep(Duration::from_millis(50));
                    DispatchOp::Dispatch(state + action, None)
                },
            )))
            .build()
            .expect("Failed to build store");

        // Dispatch multiple actions quickly
        for i in 1..=3 {
            store.dispatch(i).expect("Failed to dispatch action");
        }

        // Drop the store while actions are still being processed
        drop(store);

        // No explicit assertion needed - test passes if it doesn't hang or panic
    }

    #[test]
    fn test_store_methods_after_clone() {
        let _store = StoreBuilder::new(0)
            .with_reducer(Box::new(FnReducer::from(
                move |state: &i32, action: &i32| DispatchOp::Dispatch(state + action, None),
            )))
            .build()
            .expect("Failed to build store");

        let droppable_store = DroppableStore::new(_store);

        // Clone the store
        let store_clone = droppable_store.clone();

        // Both instances should work
        droppable_store.dispatch(1).expect("Failed to dispatch to original");
        store_clone.dispatch(2).expect("Failed to dispatch to clone");

        // Wait a bit to ensure actions are processed
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(droppable_store.get_state(), 3);
        assert_eq!(store_clone.get_state(), 3);

        // Drop original
        drop(droppable_store);

        // now Clone should be error
        let r = store_clone.dispatch(3);
        assert!(r.is_err());

        // the state should not be changed
        assert_eq!(store_clone.get_state(), 3);
    }
}
