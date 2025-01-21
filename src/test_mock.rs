use crate::{
    DispatchOp, Dispatcher, Effect, Middleware, MiddlewareOp, Reducer, StoreError, Subscriber,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct MockReducer<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    pub reduce_fn:
        Arc<Mutex<Box<dyn Fn(&State, &Action) -> DispatchOp<State, Action> + Send + Sync>>>,
    pub reduce_call_count: Arc<Mutex<usize>>,
}

impl<State, Action> MockReducer<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    pub fn new() -> Self {
        Self {
            reduce_fn: Arc::new(Mutex::new(Box::new(|state, _action| {
                DispatchOp::Dispatch(state.clone(), None)
            }))),
            reduce_call_count: Arc::new(Mutex::new(0)),
        }
    }

    #[allow(unused_mut)]
    pub fn with_reduce_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(&State, &Action) -> DispatchOp<State, Action> + Send + Sync + 'static,
    {
        *self.reduce_fn.lock().unwrap() = Box::new(f);
        self
    }
}

impl<State, Action> Reducer<State, Action> for MockReducer<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action> {
        *self.reduce_call_count.lock().unwrap() += 1;
        (self.reduce_fn.lock().unwrap())(state, action)
    }
}

pub struct MockSubscriber<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    pub on_notify_fn: Arc<Mutex<Box<dyn Fn(&State, &Action) + Send + Sync + 'static>>>,
    pub notify_call_count: Arc<Mutex<usize>>,
}

impl<State, Action> MockSubscriber<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    pub fn new() -> Self {
        Self {
            on_notify_fn: Arc::new(Mutex::new(Box::new(|_, _| ()))),
            notify_call_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn with_notify_fn<F>(self, f: F) -> Self
    where
        F: Fn(&State, &Action) + Send + Sync + 'static,
    {
        *self.on_notify_fn.lock().unwrap() = Box::new(f);
        self
    }
}

impl<State, Action> Subscriber<State, Action> for MockSubscriber<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn on_notify(&self, state: &State, action: &Action) {
        *self.notify_call_count.lock().unwrap() += 1;
        (self.on_notify_fn.lock().unwrap())(state, action)
    }
}

pub struct MockMiddleware<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    pub before_reduce_fn: Arc<
        Mutex<
            Box<
                dyn Fn(
                        &Action,
                        &State,
                        Arc<dyn Dispatcher<Action>>,
                    ) -> Result<MiddlewareOp, StoreError>
                    + Send
                    + Sync,
            >,
        >,
    >,
    pub before_effect_fn: Arc<
        Mutex<
            Box<
                dyn Fn(
                        &Action,
                        &State,
                        &mut Vec<Effect<Action>>,
                        Arc<dyn Dispatcher<Action>>,
                    ) -> Result<MiddlewareOp, StoreError>
                    + Send
                    + Sync,
            >,
        >,
    >,
    pub before_reduce_call_count: Arc<Mutex<usize>>,
    pub before_effect_call_count: Arc<Mutex<usize>>,
}

impl<State, Action> MockMiddleware<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    pub fn new() -> Self {
        Self {
            before_reduce_fn: Arc::new(Mutex::new(Box::new(|_, _, _| {
                Ok(MiddlewareOp::ContinueAction)
            }))),
            before_effect_fn: Arc::new(Mutex::new(Box::new(|_, _, _, _| {
                Ok(MiddlewareOp::ContinueAction)
            }))),
            before_reduce_call_count: Arc::new(Mutex::new(0)),
            before_effect_call_count: Arc::new(Mutex::new(0)),
        }
    }
}

impl<State, Action> Middleware<State, Action> for MockMiddleware<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn before_reduce(
        &self,
        action: &Action,
        state: &State,
        dispatcher: Arc<dyn Dispatcher<Action>>,
    ) -> Result<MiddlewareOp, StoreError> {
        *self.before_reduce_call_count.lock().unwrap() += 1;
        (self.before_reduce_fn.lock().unwrap())(action, state, dispatcher)
    }

    fn before_effect(
        &self,
        action: &Action,
        state: &State,
        effects: &mut Vec<Effect<Action>>,
        dispatcher: Arc<dyn Dispatcher<Action>>,
    ) -> Result<MiddlewareOp, StoreError> {
        *self.before_effect_call_count.lock().unwrap() += 1;
        (self.before_effect_fn.lock().unwrap())(action, state, effects, dispatcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{reducer::Reducer, Store, StoreBuilder};
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestReducer;
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            let new_state = state + action;
            DispatchOp::Dispatch(new_state, None)
        }
    }

    #[test]
    fn test_mock_reducer() {
        // given
        let mock_reducer = MockReducer::new();
        let mock_reducer_clone = mock_reducer.clone();
        let store = Store::new_with_state(Box::new(mock_reducer), 0);

        // when
        store.dispatch(5);
        store.stop();

        // then
        assert_eq!(*mock_reducer_clone.reduce_call_count.lock().unwrap(), 1);
    }

    #[test]
    fn test_mock_reducer_with() {
        // given
        let reducer_called = Arc::new(AtomicBool::new(false));
        let reducer_called_clone = reducer_called.clone();

        let mock_reducer = MockReducer::new().with_reduce_fn(move |state, action| {
            reducer_called_clone.store(true, Ordering::SeqCst);
            DispatchOp::Dispatch(*state + *action, None)
        });
        let mock_reducer_clone = mock_reducer.clone();
        let store = Store::new_with_state(Box::new(mock_reducer), 0);

        // when
        store.dispatch(5);
        store.stop();

        // then
        assert_eq!(*mock_reducer_clone.reduce_call_count.lock().unwrap(), 1);
        assert!(reducer_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_mock_subscriber() {
        // given
        let mock_subscriber = Arc::new(MockSubscriber::new());
        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_name("test_store".to_string())
            .build()
            .unwrap();
        store.add_subscriber(mock_subscriber.clone());

        // when
        store.dispatch(5);
        store.stop();

        // then
        assert_eq!(*mock_subscriber.notify_call_count.lock().unwrap(), 1);
    }

    #[test]
    fn test_mock_subscriber_with() {
        // given
        let received_state = Arc::new(Mutex::new(None));
        let received_state_clone = received_state.clone();

        let mock_subscriber = Arc::new(MockSubscriber::new().with_notify_fn(
            move |state: &i32, _action: &i32| {
                *received_state_clone.lock().unwrap() = Some(*state);
            },
        ));

        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_name("test_store".to_string())
            .build()
            .unwrap();
        store.add_subscriber(mock_subscriber.clone());

        // when
        store.dispatch(5);
        store.stop();

        // then
        assert_eq!(*received_state.lock().unwrap(), Some(5));
        assert_eq!(*mock_subscriber.notify_call_count.lock().unwrap(), 1);
    }

    #[test]
    fn test_mock_middleware() {
        // given
        let mock_middleware = Arc::new(MockMiddleware::new());

        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_name("test_store".to_string())
            .with_middleware(mock_middleware.clone())
            .build()
            .unwrap();

        // when
        store.dispatch(5);
        store.stop();

        // then
        assert_eq!(*mock_middleware.before_reduce_call_count.lock().unwrap(), 1);
        assert_eq!(*mock_middleware.before_effect_call_count.lock().unwrap(), 1);
    }

    #[test]
    fn test_mock_middleware_with() {
        // given
        let before_called = Arc::new(AtomicBool::new(false));
        let before_called_clone = before_called.clone();

        let after_called = Arc::new(AtomicBool::new(false));
        let after_called_clone = after_called.clone();

        let mock_middleware = Arc::new(MockMiddleware::new());
        {
            let middleware = mock_middleware.clone();
            *middleware.before_reduce_fn.lock().unwrap() = Box::new(move |_, _, _| {
                before_called_clone.store(true, Ordering::SeqCst);
                Ok(MiddlewareOp::ContinueAction)
            });
            *middleware.before_effect_fn.lock().unwrap() = Box::new(move |_, _, _, _| {
                after_called_clone.store(true, Ordering::SeqCst);
                Ok(MiddlewareOp::ContinueAction)
            });
        }

        let store = StoreBuilder::new()
            .with_reducer(Box::new(TestReducer))
            .with_name("test_store".to_string())
            .with_middleware(mock_middleware.clone())
            .build()
            .unwrap();

        // when
        store.dispatch(5);
        store.stop();

        // then
        assert!(before_called.load(Ordering::SeqCst));
        assert!(after_called.load(Ordering::SeqCst));
        assert_eq!(*mock_middleware.before_reduce_call_count.lock().unwrap(), 1);
        assert_eq!(*mock_middleware.before_effect_call_count.lock().unwrap(), 1);
    }
}
