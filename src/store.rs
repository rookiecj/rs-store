use crate::{BackpressurePolicy, Subscriber, Subscription};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default capacity for the channel
pub const DEFAULT_CAPACITY: usize = 16;
pub const DEFAULT_STORE_NAME: &str = "store";

/// StoreError represents an error that occurred in the store
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("dispatch error: {0}")]
    DispatchError(String),
    #[error("reducer error: {0}")]
    ReducerError(String),
    #[error("subscription error: {0}")]
    SubscriptionError(String),
    #[error("middleware error: {0}")]
    MiddlewareError(String),
    #[error("initialization error: {0}")]
    InitError(String),
    /// state update failed with context and source
    #[error("state update failed: {context}, cause: {source}")]
    StateUpdateError {
        context: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Store trait defines the interface for a Redux-like store
pub trait Store<State, Action>: Send + Sync
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    /// Get the current state
    fn get_state(&self) -> State;

    /// Dispatch an action
    fn dispatch(&self, action: Action) -> Result<(), StoreError>;

    /// Add a subscriber to the store
    /// store updates are delivered to the subscriber in same reducer thread
    fn add_subscriber(
        &self,
        subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError>;

    /// Iterate over the store's state and action pairs
    //fn iter(&self) -> impl Iterator<Item = (State, Action)>;

    /// subscribe to the store in new context
    /// store updates are delivered to the subscriber in the new context
    fn subscribed(
        &self,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError>;

    /// subscribe to the store in new context
    /// store updates are delivered to the subscriber in the new context
    ///
    /// ### Parameters
    /// * capacity: Channel buffer capacity
    /// * policy: Backpressure policy for when channel is full,
    ///     `BlockOnFull` or `DropLatestIf` is supported to prevent from dropping the ActionOp::Exit
    ///
    /// ### Return
    /// * Subscription: Subscription for the store,
    fn subscribed_with(
        &self,
        capacity: usize,
        policy: BackpressurePolicy<(Instant, State, Action)>,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError>;

    /// Stop the store
    /// when the queue is full, the send can be blocked if there is no droppable item
    fn stop(&self) -> Result<(), StoreError>;

    /// Stop the store with timeout
    fn stop_with_timeout(&self, timeout: Duration) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::StoreBuilder;
    use crate::store_droppable::DroppableStore;
    use crate::BackpressurePolicy;
    use crate::DispatchOp;
    use crate::Reducer;
    use crate::StoreImpl;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    // Mock implementations for testing
    #[derive(Debug, Clone, PartialEq)]
    struct TestState {
        counter: i32,
        message: String,
    }

    impl Default for TestState {
        fn default() -> Self {
            TestState {
                counter: 0,
                message: String::new(),
            }
        }
    }

    #[derive(Debug, Clone)]
    enum TestAction {
        Increment,
        Decrement,
        SetMessage(String),
    }

    struct TestReducer;

    impl Reducer<TestState, TestAction> for TestReducer {
        fn reduce(
            &self,
            state: &TestState,
            action: &TestAction,
        ) -> DispatchOp<TestState, TestAction> {
            match action {
                TestAction::Increment => {
                    let mut new_state = state.clone();
                    new_state.counter += 1;
                    DispatchOp::Dispatch(new_state, None)
                }
                TestAction::Decrement => {
                    let mut new_state = state.clone();
                    new_state.counter -= 1;
                    DispatchOp::Dispatch(new_state, None)
                }
                TestAction::SetMessage(msg) => {
                    let mut new_state = state.clone();
                    new_state.message = msg.clone();
                    DispatchOp::Dispatch(new_state, None)
                }
            }
        }
    }

    fn create_test_store() -> DroppableStore<TestState, TestAction> {
        DroppableStore::new(
            StoreImpl::new_with(
                TestState::default(),
                vec![Box::new(TestReducer)],
                "test-store".into(),
                16,
                BackpressurePolicy::default(),
                vec![],
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_store_get_state() {
        let store = create_test_store();
        let initial_state = store.get_state();
        assert_eq!(initial_state.counter, 0);
        assert_eq!(initial_state.message, "");
    }

    #[test]
    fn test_store_dispatch() {
        let store = create_test_store();

        // Dispatch increment action
        store.dispatch(TestAction::Increment).unwrap();
        thread::sleep(Duration::from_millis(50)); // Wait for async processing

        let state = store.get_state();
        assert_eq!(state.counter, 1);

        // Dispatch set message action
        store.dispatch(TestAction::SetMessage("Hello".into())).unwrap();
        thread::sleep(Duration::from_millis(50));

        let state = store.get_state();
        assert_eq!(state.message, "Hello");

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_multiple_actions() {
        let store = create_test_store();

        // Dispatch multiple actions
        store.dispatch(TestAction::Increment).unwrap();
        store.dispatch(TestAction::Increment).unwrap();
        store.dispatch(TestAction::SetMessage("Test".into())).unwrap();
        store.dispatch(TestAction::Decrement).unwrap();

        thread::sleep(Duration::from_millis(100));

        let final_state = store.get_state();
        assert_eq!(final_state.counter, 1);
        assert_eq!(final_state.message, "Test");

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_after_stop() {
        let store = create_test_store();
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // Dispatch should fail after stop
        let result = store.dispatch(TestAction::Increment);
        assert!(result.is_err());

        match result {
            Err(StoreError::DispatchError(_)) => (),
            _ => panic!("Expected DispatchError"),
        }
    }

    #[test]
    fn test_store_concurrent_access() {
        let store = Arc::new(create_test_store());
        let store_clone = store.clone();

        let handle = thread::spawn(move || {
            for _ in 0..5 {
                store_clone.dispatch(TestAction::Increment).unwrap();
                thread::sleep(Duration::from_millis(10));
            }
        });

        for _ in 0..5 {
            store.dispatch(TestAction::Decrement).unwrap();
            thread::sleep(Duration::from_millis(10));
        }

        handle.join().unwrap();
        thread::sleep(Duration::from_millis(100));

        let final_state = store.get_state();
        // Final counter should be 0 (5 increments and 5 decrements)
        assert_eq!(final_state.counter, 0);

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_builder_configurations() {
        let store = StoreBuilder::new(TestState::default())
            .with_reducer(Box::new(TestReducer))
            .with_name("custom-store".into())
            .with_capacity(32)
            .with_policy(BackpressurePolicy::DropLatest)
            .build()
            .unwrap();

        store.dispatch(TestAction::Increment).unwrap();
        thread::sleep(Duration::from_millis(50));

        let state = store.get_state();
        assert_eq!(state.counter, 1);

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_error_handling() {
        let store = create_test_store();
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // Test various error conditions
        let dispatch_result = store.dispatch(TestAction::Increment);
        // println!("dispatch_result: {:?}", dispatch_result);
        // dispatch_result: Err(DispatchError("Dispatch channel is closed"))
        assert!(matches!(dispatch_result, Err(StoreError::DispatchError(_))));

        // Test that the store remains in a consistent state after errors
        let state = store.get_state();
        assert_eq!(state.counter, 0);
    }
}
