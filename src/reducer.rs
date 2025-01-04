use crate::effect::Effect;

/// determine if the action should be dispatched or not
pub enum DispatchOp<State, Action> {
    /// Dispatch new state
    Dispatch(State, Option<Effect<Action>>),
    /// Keep new state but do not dispatch
    Keep(State, Option<Effect<Action>>),
}

/// Reducer reduces the state based on the action.
pub trait Reducer<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action>;
}

/// FnReducer is a reducer that is created from a function.
pub struct FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    func: F,
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action> Reducer<State, Action> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action> {
        (self.func)(state, action)
    }
}

impl<F, State, Action> From<F> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    fn from(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriber::Subscriber;
    use crate::StoreBuilder;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_store_continues_after_reducer_panic() {
        // given

        // A reducer that panics on specific action value
        struct PanicOnValueReducer {
            panic_on: i32,
        }

        impl Reducer<i32, i32> for PanicOnValueReducer {
            fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
                if *action == self.panic_on {
                    // Catch the panic and return current state
                    let result = std::panic::catch_unwind(|| {
                        panic!("Intentional panic on action {}", action);
                    });
                    // keep state if panic
                    if result.is_err() {
                        return DispatchOp::Keep(*state, None);
                    }
                }
                // Normal operation for other actions
                DispatchOp::Dispatch(state + action, None)
            }
        }

        // Create store with our test reducer
        let reducer = Box::new(PanicOnValueReducer { panic_on: 42 });
        let store = StoreBuilder::new(reducer).build().unwrap();

        // Track state changes
        let state_changes = Arc::new(Mutex::new(Vec::new()));
        let state_changes_clone = state_changes.clone();

        struct TestSubscriber {
            state_changes: Arc<Mutex<Vec<i32>>>,
        }
        impl Subscriber<i32, i32> for TestSubscriber {
            fn on_notify(&self, state: &i32, _action: &i32) {
                self.state_changes.lock().unwrap().push(*state);
            }
        }
        let subscriber = Arc::new(TestSubscriber {
            state_changes: state_changes_clone,
        });
        store.add_subscriber(subscriber);

        // then
        // Test sequence of actions
        store.dispatch(1).unwrap(); // Should work: 0 -> 1
        store.dispatch(42).unwrap(); // Should panic but be caught: stays at 1
        store.dispatch(2).unwrap(); // Should work: 1 -> 3

        // Give time for all actions to be processed
        store.stop();

        // then
        // Verify final state
        assert_eq!(store.get_state(), 3);
        // Verify state change history
        let changes = state_changes.lock().unwrap();
        assert_eq!(&*changes, &vec![1, 3]); // Should only have non-panic state changes
    }

    #[test]
    fn test_multiple_reducers_continue_after_panic() {
        // given
        struct PanicReducer;
        struct NormalReducer;

        impl Reducer<i32, i32> for PanicReducer {
            fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
                let result = std::panic::catch_unwind(|| {
                    panic!("Always panic!");
                });
                // keep state if panic
                if result.is_err() {
                    return DispatchOp::Keep(*state, None);
                }
                DispatchOp::Dispatch(state + action, None)
            }
        }

        impl Reducer<i32, i32> for NormalReducer {
            fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
                DispatchOp::Dispatch(state + action, None)
            }
        }

        // Create store with both reducers
        let store = StoreBuilder::new(Box::new(PanicReducer))
            .add_reducer(Box::new(NormalReducer))
            .build()
            .unwrap();

        // when
        // Dispatch actions
        store.dispatch(1).unwrap();
        store.dispatch(2).unwrap();

        store.stop();

        // then
        // Even though PanicReducer panics, NormalReducer should still process actions
        assert_eq!(store.get_state(), 3);
    }
}
