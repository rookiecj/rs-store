use crate::effect::Effect;
use std::sync::Arc;

/// determine if the action should be dispatched or not
pub enum DispatchOp<State, Action> {
    /// Dispatch new state with effects
    /// since 3.0.0
    Dispatch(State, Vec<Effect<Action>>),
    /// Keep new state but do not dispatch
    Keep(State, Vec<Effect<Action>>),
}

/// Reducer reduces the state based on the action.
pub trait Reducer<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action>;
}

/// ReducerChain chains reducers together sequentially.
/// The first reducer in the vector becomes the first reducer in the chain.
/// Execution order: first reducer -> second reducer -> ... -> last reducer
pub struct ReducerChain<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    reducer: Arc<dyn Reducer<State, Action> + Send + Sync>,
    next: Option<Box<ReducerChain<State, Action>>>,
}

impl<State, Action> ReducerChain<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    /// Create a new reducer chain with a single reducer
    pub fn new(reducer: Arc<dyn Reducer<State, Action> + Send + Sync>) -> Self {
        Self {
            reducer,
            next: None,
        }
    }

    /// Chain another reducer to this chain
    pub fn chain(mut self, reducer: Arc<dyn Reducer<State, Action> + Send + Sync>) -> Self {
        if let Some(ref mut next) = self.next {
            // Recursively chain to the end
            *next = Box::new(next.as_ref().clone().chain(reducer));
        } else {
            // add at tail of the chain
            self.next = Some(Box::new(ReducerChain::new(reducer)));
        }
        self
    }

    /// Create a reducer chain from a vector of reducers
    /// The first reducer in the vector becomes the first reducer in the chain.
    pub fn from_vec(reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>) -> Option<Self> {
        if reducers.is_empty() {
            return None;
        }

        let mut iter = reducers.into_iter();
        let mut tail = ReducerChain::new(Arc::from(iter.next()?));

        for reducer in iter {
            tail = tail.chain(Arc::from(reducer));
        }

        Some(tail)
    }

    /// Execute the reducer chain, starting from the first reducer
    /// Returns DispatchOp with final state and accumulated effects
    pub fn execute(&self, state: &State, action: &Action) -> DispatchOp<State, Action> {
        // Continue with next reducer if exists
        if let Some(ref next) = self.next {
            let mut accum_effects = Vec::new();
            // Execute current reducer
            let dispatch_op = self.reducer.reduce(state, action);
            let next_op = match dispatch_op {
                DispatchOp::Dispatch(intermediate_state, effects) => {
                    accum_effects.extend(effects);
                    // Continue with next reducer
                    next.execute(&intermediate_state, action)
                }
                DispatchOp::Keep(intermediate_state, effects) => {
                    accum_effects.extend(effects);
                    // Continue with next reducer
                    next.execute(&intermediate_state, action)
                }
            };
            // The last reducer's decision wins (next executes after current, so next_dispatch takes precedence)
            match next_op {
                DispatchOp::Dispatch(final_state, effects) => {
                    accum_effects.extend(effects);
                    DispatchOp::Dispatch(final_state, accum_effects)
                }
                DispatchOp::Keep(final_state, effects) => {
                    accum_effects.extend(effects);
                    DispatchOp::Keep(final_state, accum_effects)
                }
            }
        } else {
            self.reducer.reduce(state, action)
        }
    }
}

// Implement Clone for ReducerChain to support recursive chaining
impl<State, Action> Clone for ReducerChain<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    fn clone(&self) -> Self {
        Self {
            reducer: self.reducer.clone(),
            next: self.next.as_ref().map(|n| Box::new(n.as_ref().clone())),
        }
    }
}

impl<State, Action> Reducer<State, Action> for ReducerChain<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action> {
        self.execute(state, action)
    }
}

/// FnReducer is a reducer that is created from a function.
pub struct FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    func: F,
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action> Reducer<State, Action> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action> {
        (self.func)(state, action)
    }
}

impl<F, State, Action> From<F> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Send + Sync + Clone,
    Action: Send + Sync + std::fmt::Debug + 'static,
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
    use std::thread;

    struct TestSubscriber {
        state_changes: Arc<Mutex<Vec<i32>>>,
    }

    impl Subscriber<i32, i32> for TestSubscriber {
        fn on_notify(&self, state: &i32, _action: &i32) {
            self.state_changes.lock().unwrap().push(*state);
        }
    }

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
                        return DispatchOp::Keep(*state, vec![]);
                    }
                }
                // Normal operation for other actions
                DispatchOp::Dispatch(state + action, vec![])
            }
        }

        // Create store with our test reducer
        let reducer = Box::new(PanicOnValueReducer { panic_on: 42 });
        let store = StoreBuilder::new_with_reducer(0, reducer).build().unwrap();

        // Track state changes
        let state_changes = Arc::new(Mutex::new(Vec::new()));
        let state_changes_clone = state_changes.clone();

        let subscriber = Arc::new(TestSubscriber {
            state_changes: state_changes_clone,
        });
        store.add_subscriber(subscriber).unwrap();

        // then
        // Test sequence of actions
        store.dispatch(1).unwrap(); // Should work: 0 -> 1
        store.dispatch(42).unwrap(); // Should panic but be caught: stays at 1
        store.dispatch(2).unwrap(); // Should work: 1 -> 3

        // Give time for all actions to be processed
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

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
                    return DispatchOp::Keep(*state, vec![]);
                }
                DispatchOp::Dispatch(state + action, vec![])
            }
        }

        impl Reducer<i32, i32> for NormalReducer {
            fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
                DispatchOp::Dispatch(state + action, vec![])
            }
        }

        // Create store with both reducers
        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(PanicReducer))
            .add_reducer(Box::new(NormalReducer))
            .build()
            .unwrap();

        // when
        // Dispatch actions
        store.dispatch(1).unwrap();
        store.dispatch(2).unwrap();

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        // Even though PanicReducer panics, NormalReducer should still process actions
        assert_eq!(store.get_state(), 3);
    }

    #[test]
    fn test_fn_reducer_basic() {
        // given
        let reducer = FnReducer::from(|state: &i32, action: &i32| {
            DispatchOp::Dispatch(state + action, vec![])
        });
        let store = StoreBuilder::new_with_reducer(0, Box::new(reducer)).build().unwrap();

        // when
        store.dispatch(5).unwrap();
        store.dispatch(3).unwrap();
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        assert_eq!(store.get_state(), 8); // 0 + 5 + 3 = 8
    }

    #[test]
    fn test_fn_reducer_with_effect() {
        // given
        #[derive(Clone, Debug)]
        enum Action {
            AddWithEffect(i32),
            Add(i32),
        }

        let reducer = FnReducer::from(|state: &i32, action: &Action| {
            match action {
                Action::AddWithEffect(i) => {
                    let new_state = state + i;
                    let effect = Effect::Action(Action::Add(40)); // Effect that adds 40 more
                    DispatchOp::Dispatch(new_state, vec![effect])
                }
                Action::Add(i) => {
                    let new_state = state + i;
                    DispatchOp::Dispatch(new_state, vec![])
                }
            }
        });
        let store = StoreBuilder::new_with_reducer(0, Box::new(reducer)).build().unwrap();

        // when
        store.dispatch(Action::AddWithEffect(2)).unwrap();
        thread::sleep(std::time::Duration::from_millis(1000)); // Wait for effect to be processed
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        // Initial state(0) + action(2) + effect(40) = 42
        assert_eq!(store.get_state(), 42);
    }

    #[test]
    fn test_fn_reducer_keep_state() {
        // given
        let reducer = FnReducer::from(|state: &i32, action: &i32| {
            if *action < 0 {
                // Keep current state for negative actions
                DispatchOp::Keep(*state, vec![])
            } else {
                DispatchOp::Dispatch(state + action, vec![])
            }
        });
        let store = StoreBuilder::new_with_reducer(0, Box::new(reducer)).build().unwrap();

        // Track state changes
        let state_changes = Arc::new(Mutex::new(Vec::new()));
        let state_changes_clone = state_changes.clone();

        let subscriber = Arc::new(TestSubscriber {
            state_changes: state_changes_clone,
        });
        store.add_subscriber(subscriber).unwrap();

        // when
        store.dispatch(5).unwrap(); // Should change state
        store.dispatch(-3).unwrap(); // Should keep state
        store.dispatch(2).unwrap(); // Should change state
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        assert_eq!(store.get_state(), 7); // 0 + 5 + 2 = 7
        let changes = state_changes.lock().unwrap();
        assert_eq!(&*changes, &vec![5, 7]); // Only two state changes should be recorded
    }

    #[test]
    fn test_multiple_fn_reducers() {
        // given
        let add_reducer = FnReducer::from(|state: &i32, action: &i32| {
            DispatchOp::Dispatch(state + action, vec![])
        });
        let double_reducer =
            FnReducer::from(|state: &i32, _action: &i32| DispatchOp::Dispatch(state * 2, vec![]));

        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(add_reducer))
            .add_reducer(Box::new(double_reducer))
            .build()
            .unwrap();

        // when
        store.dispatch(3).unwrap(); // (((0)  +3) *2) = 6
        store.dispatch(15).unwrap(); // (((6) +15) *2) = 42
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        assert_eq!(store.get_state(), 42);
    }
}
