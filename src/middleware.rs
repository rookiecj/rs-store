use std::sync::Arc;

use crate::reducer::DispatchOp;
use crate::store::StoreError;

/// Type alias for a middleware function.
///
/// ## Arguments
/// - `state`: Current state (reference)
/// - `action`: Action to process (reference)
///
/// ## Returns
/// - `Ok(DispatchOp<State, Action>)`: dispatch operation containing the next state and any effects
/// - `Err(StoreError)`: if an error occurs during processing
pub type MiddlewareFn<State, Action> = Arc<
    dyn Fn(&State, &Action) -> Result<DispatchOp<State, Action>, StoreError>
        + Send
        + Sync
        + 'static,
>;

/// Factory trait to create a [`MiddlewareFn`] from an inner [`MiddlewareFn`].
pub trait MiddlewareFnFactory<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn create(&self, inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action>;
}

#[cfg(test)]
mod tests {
    use crate::{BackpressurePolicy, DispatchOp, Effect, Reducer, StoreImpl};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::*;

    struct TestReducer;
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, _state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            DispatchOp::Dispatch(*action, vec![])
        }
    }

    struct LoggerMiddleware {
        logs: Arc<Mutex<Vec<String>>>,
        #[allow(dead_code)]
        errors: Arc<Mutex<Vec<StoreError>>>,
    }

    impl LoggerMiddleware {
        fn new() -> Self {
            Self {
                logs: Arc::new(Mutex::new(vec![])),
                errors: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    impl<State, Action> MiddlewareFnFactory<State, Action> for LoggerMiddleware
    where
        State: Send + Sync + Clone + 'static,
        Action: Send + Sync + Clone + std::fmt::Debug + 'static,
    {
        fn create(&self, inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action> {
            let logs_clone = self.logs.clone();
            Arc::new(move |state: &State, action: &Action| {
                let log = format!("before: - Action: {:?}", action);
                println!("{}", log);
                // (possible poisoned)
                logs_clone.lock().unwrap().push(log);

                // inner
                let result = inner(state, action);

                let log = format!("after: - Action: {:?}", action);
                println!("{}", log);
                logs_clone.lock().unwrap().push(log);

                // return
                result
            })
        }
    }

    #[test]
    fn test_logger_middleware() {
        let reducer = Box::new(TestReducer);
        let logger = Arc::new(LoggerMiddleware::new());
        let store = StoreImpl::new_with(
            0,
            vec![reducer],
            "test_logger_middleware".to_string(),
            2,
            BackpressurePolicy::BlockOnFull,
            vec![logger.clone()],
        )
        .unwrap();

        let _ = store.dispatch(1);

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        assert_eq!(logger.logs.lock().unwrap().len(), 2);
        assert!(logger.logs.lock().unwrap().first().unwrap().contains("before: - Action"));
        assert!(logger.logs.lock().unwrap().last().unwrap().contains("after: - Action"));
    }

    #[derive(Debug, Clone)]
    enum MiddlewareAction {
        ReqThisAction(i32),
        ResponseAsThis(i32),
    }
    struct MiddlewareReducer {}
    impl MiddlewareReducer {
        fn new() -> Self {
            Self {}
        }
    }
    impl Reducer<i32, MiddlewareAction> for MiddlewareReducer {
        fn reduce(
            &self,
            _state: &i32,
            action: &MiddlewareAction,
        ) -> DispatchOp<i32, MiddlewareAction> {
            match action {
                MiddlewareAction::ReqThisAction(value) => DispatchOp::Dispatch(*value, vec![]),
                MiddlewareAction::ResponseAsThis(value) => DispatchOp::Dispatch(*value, vec![]),
            }
        }
    }

    struct MiddlewareBeforeDispatch;
    impl MiddlewareBeforeDispatch {
        fn new() -> Arc<Self> {
            Arc::new(Self {})
        }
    }
    impl<State> MiddlewareFnFactory<State, MiddlewareAction> for MiddlewareBeforeDispatch
    where
        State: Send + Sync + Clone + 'static,
    {
        fn create(
            &self,
            inner: MiddlewareFn<State, MiddlewareAction>,
        ) -> MiddlewareFn<State, MiddlewareAction> {
            Arc::new(move |state: &State, action: &MiddlewareAction| {
                match action {
                    MiddlewareAction::ReqThisAction(v) => {
                        // Create an effect that dispatches ResponseAsThis(v + 1)
                        let value = *v + 1;
                        let effect = Effect::Action(MiddlewareAction::ResponseAsThis(value));
                        // Don't process the original action, just return the effect
                        Ok(DispatchOp::Keep(state.clone(), vec![effect]))
                    }
                    _ => {
                        // For other actions, pass through to next middleware
                        inner(state, action)
                    }
                }
            })
        }
    }

    #[test]
    fn test_middleware_before_reduce() {
        // given
        let reducer = Box::new(MiddlewareReducer::new());
        let effect_middleware = MiddlewareBeforeDispatch::new();
        let store = StoreImpl::new_with(
            0,
            vec![reducer],
            "test_middleware_before_reduce".to_string(),
            2,
            BackpressurePolicy::BlockOnFull,
            vec![effect_middleware],
        )
        .unwrap();

        // when

        let _ = store.dispatch(MiddlewareAction::ReqThisAction(42));
        // at this point, the state should not be changed
        assert_eq!(store.get_state(), 0);

        // give time to the effect
        thread::sleep(Duration::from_millis(1000));
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // the state should be changed
        assert_eq!(store.get_state(), 43);
    }

    #[derive(Debug, Clone)]
    struct EffectState {
        value: i32,
    }
    impl Default for EffectState {
        fn default() -> Self {
            Self { value: 0 }
        }
    }

    #[derive(Debug, Clone)]
    enum EffectAction {
        ActionProduceEffectFunction(i32),
        ResponseForTheEffect(i32),
    }
    struct EffectReducer {}
    impl EffectReducer {
        fn new() -> Self {
            Self {}
        }
    }
    impl Reducer<EffectState, EffectAction> for EffectReducer {
        fn reduce(
            &self,
            _state: &EffectState,
            action: &EffectAction,
        ) -> DispatchOp<EffectState, EffectAction> {
            match action {
                EffectAction::ActionProduceEffectFunction(value) => {
                    let new_state = EffectState { value: *value };

                    // produce an effect function that returns an Action
                    let value_clone = *value;
                    let effect = Effect::Function(
                        "key1".to_string(),
                        Box::new(move || {
                            println!("effect: {:?}", value_clone);

                            // do long running task

                            // return the result as an Action so it can be dispatched
                            let action = EffectAction::ResponseForTheEffect(value_clone + 1);
                            Ok(Box::new(action) as Box<dyn std::any::Any>)
                        }),
                    );
                    DispatchOp::Dispatch(new_state, vec![effect])
                }
                EffectAction::ResponseForTheEffect(value) => {
                    let new_state = EffectState { value: *value };
                    DispatchOp::Dispatch(new_state, vec![])
                }
            }
        }
    }

    struct EffectMiddleware;
    impl EffectMiddleware {
        fn new() -> Self {
            Self {}
        }
    }

    impl MiddlewareFnFactory<EffectState, EffectAction> for EffectMiddleware {
        fn create(
            &self,
            inner: MiddlewareFn<EffectState, EffectAction>,
        ) -> MiddlewareFn<EffectState, EffectAction> {
            Arc::new(move |state: &EffectState, action: &EffectAction| {
                // call inner middleware/reducer
                let dispatch_op = inner(state, action)?;

                // Pass through the DispatchOp - effect handling is done in do_effect
                Ok(dispatch_op)
            })
        }
    }

    #[test]
    fn test_effect_middleware() {
        // given
        let reducer = Box::new(EffectReducer::new());
        // when
        let effect_middleware = Arc::new(EffectMiddleware::new());
        let store = StoreImpl::new_with(
            EffectState::default(),
            vec![reducer],
            "test_effect_middleware".to_string(),
            2,
            BackpressurePolicy::BlockOnFull,
            vec![effect_middleware.clone()],
        )
        .unwrap();

        let _ = store.dispatch(EffectAction::ActionProduceEffectFunction(42));

        // the effect produces another effect
        thread::sleep(Duration::from_millis(1000));

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        assert_eq!(store.get_state().value, 43);
    }

    #[test]
    fn test_middleware_poisoned() {
        let reducer = Box::new(TestReducer);
        // Add logger middleware
        let logger = Arc::new(LoggerMiddleware::new());
        let store = StoreImpl::new_with(
            0,
            vec![reducer],
            "test_middleware_poisoned".to_string(),
            2,
            BackpressurePolicy::BlockOnFull,
            vec![logger.clone()],
        )
        .unwrap();

        // Poison the mutex by spawning a thread that panics while holding the lock
        let logger_clone = logger.clone();
        let handle = thread::spawn(move || {
            let _lock = logger_clone.logs.lock().unwrap();
            panic!("Intentionally poison the mutex");
        });

        // Wait for the thread to panic and poison the mutex
        let _ = handle.join().expect_err("Thread should panic");

        // Verify the mutex is poisoned
        assert!(logger.logs.lock().is_err());

        let _ = store.dispatch(1);

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        match logger.logs.lock() {
            Ok(_) => panic!("Mutex should be poisoned"),
            Err(err) => {
                println!("Mutex is poisoned as expected");
                assert_eq!(err.get_ref().len(), 0);
            }
        };
    }
}
