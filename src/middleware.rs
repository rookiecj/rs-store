use std::sync::Arc;
use std::time::Instant;

use crate::store::StoreError;
use crate::Dispatcher;
use crate::Effect;

/// Context passed to middleware during processing
pub struct MiddlewareContext<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    /// The action being processed
    pub action: Action,
    /// time the action received
    pub action_at: Instant,
    /// The accumulated state as a result of processing middlewares and reducers
    pub state: State,
    /// The accumulated effects generated during processing
    pub effects: Vec<Effect<Action>>,
    /// Dispatcher to allow middleware to dispatch additional actions
    pub dispatcher: Arc<dyn Dispatcher<Action>>,
}

/// Type alias for NewMiddleware function
/// This is a boxed function that takes a mutable reference to MiddlewareContext and returns a Result
///
/// ## Arguments
/// - ctx: &mut MiddlewareContext<State, Action> - the context containing action, state, and dispatcher,
///   the middlewares can modify the context (e.g., action, state) as a result of processing
///
/// ## Returns
/// - Ok(bool): true if the action should continue to be processed, false to stop processing
/// - Err(StoreError): if an error occurs during processing
pub type MiddlewareFn<State, Action> = Arc<
    dyn Fn(&mut MiddlewareContext<State, Action>) -> Result<bool, StoreError>
        + Send
        + Sync
        + 'static,
>;

/// Factory trait to create NewMiddlewareFn from an inner NewMiddlewareFn
pub trait MiddlewareFnFactory<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn create(&self, inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action>;
}

#[cfg(test)]
mod tests {
    use crate::{BackpressurePolicy, DispatchOp, Dispatcher, Reducer, StoreBuilder, StoreImpl};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::*;

    struct TestReducer;
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, _state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            let new_state = *action;
            DispatchOp::Dispatch(new_state, vec![])
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
            Arc::new(move |ctx: &mut MiddlewareContext<State, Action>| {
                let log = format!("before: - Action: {:?}", ctx.action);
                println!("{}", log);
                // (possible poisoned)
                logs_clone.lock().unwrap().push(log);

                // inner
                let result = inner(ctx);

                let log = format!("after: - Action: {:?}", ctx.action);
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

    struct ChainMiddlewareContinueAction;
    impl ChainMiddlewareContinueAction {
        fn new() -> Self {
            Self {}
        }
    }

    impl<State, Action> MiddlewareFnFactory<State, Action> for ChainMiddlewareContinueAction
    where
        State: Send + Sync + Clone + 'static,
        Action: Send + Sync + Clone + std::fmt::Debug + 'static,
    {
        fn create(&self, inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action> {
            Arc::new(move |ctx: &mut MiddlewareContext<State, Action>| {
                // Continue processing
                inner(ctx)
            })
        }
    }

    struct ChainMiddlewareDoneAction;
    impl ChainMiddlewareDoneAction {
        fn new() -> Self {
            Self {}
        }
    }

    impl<State, Action> MiddlewareFnFactory<State, Action> for ChainMiddlewareDoneAction
    where
        State: Send + Sync + Clone + 'static,
        Action: Send + Sync + Clone + std::fmt::Debug + 'static,
    {
        fn create(&self, _inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action> {
            Arc::new(move |_ctx: &mut MiddlewareContext<State, Action>| {
                // Skip further processing
                Ok(false)
            })
        }
    }

    #[test]
    fn test_middleware_done_action() {
        // given
        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(TestReducer))
            .add_middleware(Arc::new(ChainMiddlewareDoneAction::new()))
            .add_middleware(Arc::new(ChainMiddlewareContinueAction::new()))
            .build();
        assert!(store.is_ok());
        let store = store.unwrap();

        // when
        let _ = store.dispatch(42);
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // then
        // the state should not be changed
        assert_eq!(store.get_state(), 0);
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
                MiddlewareAction::ReqThisAction(value) => {
                    let new_state = *value;
                    DispatchOp::Dispatch(new_state, vec![])
                }
                MiddlewareAction::ResponseAsThis(value) => {
                    let new_state = *value;
                    DispatchOp::Dispatch(new_state, vec![])
                }
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
            Arc::new(
                move |ctx: &mut MiddlewareContext<State, MiddlewareAction>| {
                    match &ctx.action {
                        MiddlewareAction::ReqThisAction(v) => {
                            // do async
                            // ReqAsync -> Response
                            let v = v.clone();
                            ctx.dispatcher.dispatch_thunk(Box::new(move |dispatcher| {
                                let _ =
                                    dispatcher.dispatch(MiddlewareAction::ResponseAsThis(v + 1));
                            }));
                        }
                        _ => {}
                    }

                    // continue to next middleware
                    inner(ctx)
                },
            )
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
                    let new_state = EffectState {
                        value: value.clone(),
                    };

                    // produce an effect function
                    let value_clone = value.clone();
                    let effect = Effect::Function(
                        "key1".to_string(),
                        Box::new(move || {
                            println!("effect: {:?}", value_clone);

                            // do long running task

                            // and return result of the task
                            let new_result = Box::new(value_clone + 1);
                            Ok(new_result)
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
            Arc::new(
                move |ctx: &mut MiddlewareContext<EffectState, EffectAction>| {
                    // call inner middleware/reducer
                    let op = inner(ctx)?;

                    while ctx.effects.len() > 0 {
                        let effect = ctx.effects.remove(0);
                        match effect {
                            Effect::Function(tok, effect_fn) => {
                                // do async
                                ctx.dispatcher.dispatch_thunk(Box::new(move |dispatcher| {
                                    // do side effect
                                    let result = effect_fn();

                                    // send response
                                    match result {
                                        Ok(new_value) => {
                                            // ensure the result type is i32
                                            assert_eq!(tok, "key1");
                                            // it is almost safe to cast.
                                            let new_result = new_value.downcast::<i32>().unwrap();
                                            // and can determine which action can be dispatched
                                            dispatcher
                                                .dispatch(EffectAction::ResponseForTheEffect(
                                                    *new_result,
                                                ))
                                                .expect("no dispatch failed");
                                        }
                                        Err(e) => {
                                            eprintln!("Error: {:?}", e);
                                        }
                                    }
                                }));
                            }
                            _ => {
                                //assert!(false);
                                return Err(StoreError::MiddlewareError(
                                    "Unexpected effect type".to_string(),
                                ));
                            }
                        }
                    }

                    Ok(op)
                },
            )
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
