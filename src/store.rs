use crate::channel::{BackpressureChannel, BackpressurePolicy, SenderChannel};
use crate::dispatcher::Dispatcher;
use crate::metrics::StoreMetrics;
use crate::middleware::Middleware;
use crate::{
    DispatchOp, Effect, MiddlewareOp, Reducer, Selector, SelectorSubscriber, Subscriber,
    Subscription,
};
use fmt::Debug;
use rusty_pool::ThreadPool;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default capacity for the channel
pub const DEFAULT_CAPACITY: usize = 16;

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

#[derive(Debug)]
pub(crate) enum ActionOp<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    Action(Action),
    Exit,
}

/// Store is a simple implementation of a Redux store
pub struct Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    #[allow(dead_code)]
    name: String,
    state: Mutex<State>,
    pub(crate) reducers: Mutex<Vec<Box<dyn Reducer<State, Action> + Send + Sync>>>,
    pub(crate) subscribers: Arc<Mutex<Vec<Arc<dyn Subscriber<State, Action> + Send + Sync>>>>,
    pub(crate) dispatch_tx: Mutex<Option<SenderChannel<Action>>>,
    middlewares: Mutex<Vec<Arc<Mutex<dyn Middleware<State, Action> + Send + Sync>>>>,
    metrics: Option<Arc<dyn StoreMetrics + Send + Sync>>,
    pub(crate) pool: Mutex<Option<ThreadPool>>,
    #[cfg(feature = "notify-channel")]
    notify_tx: Mutex<Option<SenderChannel<(State, Action)>>>,
}

impl<State, Action> Default for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn default() -> Store<State, Action> {
        Store {
            name: "store".to_string(),
            state: Default::default(),
            reducers: Mutex::new(Vec::default()),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            dispatch_tx: Mutex::new(None),
            middlewares: Mutex::new(Vec::default()),
            metrics: None,
            pool: Mutex::new(Some(
                rusty_pool::Builder::new().name("store-pool".to_string()).build(),
            )),
            #[cfg(feature = "notify-channel")]
            notify_tx: Mutex::new(None),
        }
    }
}

struct SubscriptionImpl {
    unsubscribe: Box<dyn Fn() + Send + Sync>,
}

impl Subscription for SubscriptionImpl {
    fn unsubscribe(&self) {
        (self.unsubscribe)();
    }
}

impl<State, Action> Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    /// create a new store with a reducer
    pub fn new(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
    ) -> Arc<Store<State, Action>> {
        Self::new_with_state(reducer, Default::default())
    }

    /// create a new store with a reducer and an initial state
    pub fn new_with_state(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
        state: State,
    ) -> Arc<Store<State, Action>> {
        Self::new_with_name(reducer, state, "store".into()).unwrap()
    }

    /// create a new store with name
    pub fn new_with_name(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
        state: State,
        name: String,
    ) -> Result<Arc<Store<State, Action>>, StoreError> {
        Self::new_with(
            vec![reducer],
            state,
            name,
            DEFAULT_CAPACITY,
            BackpressurePolicy::default(),
            vec![],
            None,
        )
    }

    /// create a new store with name
    pub fn new_with(
        reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
        state: State,
        name: String,
        capacity: usize,
        policy: BackpressurePolicy,
        middlewares: Vec<Arc<Mutex<dyn Middleware<State, Action> + Send + Sync>>>,
        metrics: Option<Arc<dyn StoreMetrics + Send + Sync>>,
    ) -> Result<Arc<Store<State, Action>>, StoreError> {
        let (tx, rx) = BackpressureChannel::<Action>::pair_with_metrics(
            capacity,
            policy.clone(),
            metrics.clone(),
        );

        #[cfg(feature = "notify-channel")]
        let (notify_tx, notify_rx) = BackpressureChannel::<(State, Action)>::pair_with_metrics(
            capacity,
            policy.clone(),
            metrics.clone(),
        );

        let store = Store {
            name: name.clone(),
            state: Mutex::new(state),
            reducers: Mutex::new(reducers),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            middlewares: Mutex::new(middlewares),
            dispatch_tx: Mutex::new(Some(tx)),
            metrics: metrics.clone(),
            pool: Mutex::new(Some(
                rusty_pool::Builder::new().name(format!("{}-pool", name)).build(),
            )),
            #[cfg(feature = "notify-channel")]
            notify_tx: Mutex::new(Some(notify_tx)),
        };

        // start a thread in which the store will listen for actions
        let rx_store = Arc::new(store);
        let tx_store = rx_store.clone();

        // notify 스레드
        #[cfg(feature = "notify-channel")]
        {
            let notify_tx_store = tx_store.clone();
            let notify_store = rx_store.clone();
            notify_tx_store.pool.lock().unwrap().as_ref().unwrap().execute(move || {
                #[cfg(dev)]
                eprintln!("store: notify thread started");

                while let Some(msg) = notify_rx.recv() {
                    match msg {
                        ActionOp::Action((state, action)) => {
                            let start = Instant::now();

                            let subscribers = notify_store.subscribers.lock().unwrap().clone();
                            for subscriber in subscribers.iter() {
                                subscriber.on_notify(&state, &action);
                            }

                            if let Some(metrics) = &notify_store.metrics {
                                let duration = start.elapsed();
                                metrics.subscriber_notified(
                                    Some(&state),
                                    subscribers.len(),
                                    duration,
                                );
                            }
                        }
                        ActionOp::Exit => {
                            #[cfg(dev)]
                            eprintln!("store: Notify channel closed");
                            break;
                        }
                    }
                }

                #[cfg(dev)]
                eprintln!("store: notify thread exit");
            });
        }

        // reducer 스레드
        tx_store.pool.lock().unwrap().as_ref().unwrap().execute(move || {
            #[cfg(dev)]
            eprintln!("store: reducer thread started");

            while let Some(action) = rx.recv() {
                if let Some(metrics) = &rx_store.metrics {
                    metrics.action_received(Some(&action));
                }

                match action {
                    ActionOp::Action(action) => {
                        let the_dispatcher = Arc::new(rx_store.clone());

                        // do reduce
                        let current_state = rx_store.state.lock().unwrap().clone();
                        let (need_dispatch, new_state, effects) =
                            rx_store.do_reduce(&action, current_state, the_dispatcher.clone());
                        *rx_store.state.lock().unwrap() = new_state.clone();

                        // do effects remain
                        if let Some(mut effects) = effects {
                            rx_store.do_effect(
                                &action,
                                &new_state,
                                &mut effects,
                                the_dispatcher.clone(),
                            );
                        }

                        // do notify subscribers
                        if need_dispatch {
                            rx_store.do_notify(&action, &new_state, the_dispatcher.clone());
                        }
                    }
                    ActionOp::Exit => {
                        rx_store.on_close();
                        #[cfg(dev)]
                        eprintln!("store: reducer action exit");
                        break;
                    }
                }
            }

            #[cfg(dev)]
            eprintln!("store: reducer thread exit");
        });

        Ok(tx_store)
    }

    /// get the lastest state(for debugging)
    ///
    /// prefer to use `subscribe` to get the state
    pub fn get_state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// add a   reducer to the store
    pub fn add_reducer(&self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
        self.reducers.lock().unwrap().push(reducer);
    }

    /// add a subscriber to the store
    pub fn add_subscriber(
        &self,
        subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Box<dyn Subscription> {
        // append a subscriber
        self.subscribers.lock().unwrap().push(subscriber.clone());

        // disposer for the subscriber
        let subscribers = self.subscribers.clone();
        Box::new(SubscriptionImpl {
            unsubscribe: Box::new(move || {
                let mut subscribers = subscribers.lock().unwrap();
                subscribers.retain(|s| !Arc::ptr_eq(s, &subscriber));
            }),
        })
    }

    /// 셀렉터를 사용하여 상태 변경을 구독
    pub fn subscribe_with_selector<Select, Output, F>(
        &self,
        selector: Select,
        on_change: F,
    ) -> Box<dyn Subscription>
    where
        State: Default + Send + Sync + Clone,
        Action: Send + Sync + Clone,
        Select: Selector<State, Output> + Send + Sync + 'static,
        Output: PartialEq + Clone + Send + Sync + 'static,
        F: Fn(Output, Action) + Send + Sync + 'static,
    {
        let subscriber = SelectorSubscriber::new(selector, on_change);
        self.add_subscriber(Arc::new(subscriber))
    }

    /// clear all subscribers
    pub fn clear_subscribers(&self) {
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.clear();
    }

    /// do reduce
    ///
    /// ### Return
    /// * bool : true if the state to be dispatched
    /// * effects : side effects
    pub(crate) fn do_reduce(
        &self,
        action: &Action,
        mut state: State,
        dispatcher: Arc<dyn Dispatcher<Action>>,
    ) -> (bool, State, Option<Vec<Effect<Action>>>) {
        //let state = self.state.lock().unwrap().clone();

        let mut reduce_action = true;
        if !self.middlewares.lock().unwrap().is_empty() {
            let middleware_start = Instant::now();
            let mut middleware_executed = 0;
            for middleware in self.middlewares.lock().unwrap().iter() {
                middleware_executed += 1;
                match middleware.lock().unwrap().before_reduce(action, &state, dispatcher.clone()) {
                    Ok(MiddlewareOp::ContinueAction) => {
                        // continue dispatching the action
                    }
                    Ok(MiddlewareOp::DoneAction) => {
                        // stop dispatching the action
                        // last middleware wins
                        reduce_action = false;
                    }
                    Ok(MiddlewareOp::BreakChain) => {
                        // break the middleware chain
                        break;
                    }
                    Err(_e) => {
                        #[cfg(dev)]
                        eprintln!("store: Middleware error: {:?}", _e);
                        //return (false, None);
                    }
                }
            }
            if let Some(metrics) = &self.metrics {
                let middleware_duration = middleware_start.elapsed();
                metrics.middleware_executed(
                    Some(action),
                    "before_reduce",
                    middleware_executed,
                    middleware_duration,
                );
            }
        }

        let mut effects = vec![];
        let mut need_dispatch = true;
        if reduce_action {
            let reducer_start = Instant::now();

            for reducer in self.reducers.lock().unwrap().iter() {
                match reducer.reduce(&state, action) {
                    DispatchOp::Dispatch(new_state, effect) => {
                        state = new_state;
                        if let Some(effect) = effect {
                            effects.push(effect);
                        }
                        need_dispatch = true;
                    }
                    DispatchOp::Keep(new_state, effect) => {
                        // keep the state but do not dispatch
                        state = new_state;
                        if let Some(effect) = effect {
                            effects.push(effect);
                        }
                        need_dispatch = false;
                    }
                }
            }

            // reducer 실행 시간 측정 종료 및 기록
            if let Some(metrics) = &self.metrics {
                let reducer_duration = reducer_start.elapsed();
                metrics.action_reduced(Some(action), reducer_duration);
            }
        }

        //*self.state.lock().unwrap() = next_state;
        (need_dispatch, state, Some(effects))
    }

    pub(crate) fn do_effect(
        &self,
        action: &Action,
        state: &State,
        effects: &mut Vec<Effect<Action>>,
        dispatcher: Arc<dyn Dispatcher<Action>>,
    ) {
        let effect_start = Instant::now();
        if let Some(metrics) = &self.metrics {
            metrics.effect_issued(effects.len());
        }

        if !self.middlewares.lock().unwrap().is_empty() {
            let middleware_start = Instant::now();
            let mut middleware_executed = 0;
            for middleware in self.middlewares.lock().unwrap().iter() {
                middleware_executed += 1;
                match middleware.lock().unwrap().before_effect(
                    action,
                    &state,
                    effects,
                    dispatcher.clone(),
                ) {
                    Ok(MiddlewareOp::ContinueAction) => {
                        // do nothing
                    }
                    Ok(MiddlewareOp::DoneAction) => {
                        // do nothing
                    }
                    Ok(MiddlewareOp::BreakChain) => {
                        // break the middleware chain
                        break;
                    }
                    Err(_e) => {
                        #[cfg(dev)]
                        eprintln!("store: Middleware error: {:?}", _e);
                        //return (false, None);
                    }
                }
            }
            if let Some(metrics) = &self.metrics {
                let middleware_duration = middleware_start.elapsed();
                metrics.middleware_executed(
                    Some(action),
                    "before_effect",
                    middleware_executed,
                    middleware_duration,
                );
            }
        }

        while !effects.is_empty() {
            let effect = effects.remove(0);
            match effect {
                Effect::Action(a) => {
                    dispatcher.dispatch_thunk(Box::new(move |dispatcher| {
                        dispatcher.dispatch(a);
                    }));
                }
                Effect::Task(task) => {
                    dispatcher.dispatch_task(task);
                }
                Effect::Thunk(thunk) => {
                    dispatcher.dispatch_thunk(thunk);
                }
                Effect::Function(_tok, func) => {
                    dispatcher.dispatch_task(Box::new(move || {
                        // when the result of the function needs to be handled, it should be done in middleware
                        let _ = func();
                    }));
                }
            };
        }

        if let Some(metrics) = &self.metrics {
            let duration = effect_start.elapsed();
            metrics.effect_executed(effects.len(), duration);
        }
    }

    pub(crate) fn do_notify(
        &self,
        action: &Action,
        next_state: &State,
        dispatcher: Arc<dyn Dispatcher<Action>>,
    ) {
        let _notify_start = Instant::now();
        if let Some(metrics) = &self.metrics {
            metrics.state_notified(Some(next_state));
        }

        let mut need_notify = true;
        if !self.middlewares.lock().unwrap().is_empty() {
            let middleware_start = Instant::now();
            let mut middleware_executed = 0;
            for middleware in self.middlewares.lock().unwrap().iter() {
                middleware_executed += 1;
                match middleware.lock().unwrap().before_dispatch(
                    action,
                    &next_state,
                    dispatcher.clone(),
                ) {
                    Ok(MiddlewareOp::ContinueAction) => {
                        // do nothing
                    }
                    Ok(MiddlewareOp::DoneAction) => {
                        // last win
                        need_notify = false;
                    }
                    Ok(MiddlewareOp::BreakChain) => {
                        // break the middleware chain
                        break;
                    }
                    Err(_e) => {
                        #[cfg(dev)]
                        eprintln!("store: Middleware error: {:?}", _e);
                        //return (false, None);
                    }
                }
            }
            if let Some(metrics) = &self.metrics {
                let middleware_duration = middleware_start.elapsed();
                metrics.middleware_executed(
                    Some(action),
                    "before_dispatch",
                    middleware_executed,
                    middleware_duration,
                );
            }
        }

        if need_notify {
            #[cfg(feature = "notify-channel")]
            if let Some(notify_tx) = self.notify_tx.lock().unwrap().as_ref() {
                let _ = notify_tx.send(ActionOp::Action((next_state.clone(), action.clone())));
            }

            #[cfg(not(feature = "notify-channel"))]
            {
                let subscribers = self.subscribers.lock().unwrap().clone();
                for subscriber in subscribers.iter() {
                    subscriber.on_notify(&next_state, action)
                }
                if let Some(metrics) = &self.metrics {
                    let duration = _notify_start.elapsed();
                    metrics.subscriber_notified(Some(action), subscribers.len(), duration);
                }
            }
        }
    }

    fn on_close(&self) {
        #[cfg(dev)]
        eprintln!("store: on_close");

        #[cfg(feature = "notify-channel")]
        if let Some(notify_tx) = self.notify_tx.lock().unwrap().take() {
            let _ = notify_tx.send(ActionOp::Exit);
            drop(notify_tx);
        }
    }

    /// close the store
    pub fn close(&self) {
        if let Some(tx) = self.dispatch_tx.lock().unwrap().take() {
            match tx.send(ActionOp::Exit) {
                Ok(_) => {
                    #[cfg(dev)]
                    eprintln!("store: Store closed");
                }
                Err(_e) => {
                    #[cfg(dev)]
                    eprintln!("store: Error while closing Store");
                }
            }
            drop(tx);
        }
    }

    /// close the store and wait for the dispatcher to finish
    pub fn stop(&self) {
        self.close();

        // Shutdown the thread pool with timeout
        // lock pool
        let pool_took = self.pool.lock().unwrap().take();
        // unlock pool
        if let Some(pool) = pool_took {
            if cfg!(dev) {
                pool.shutdown_join();
            } else {
                pool.shutdown_join_timeout(Duration::from_secs(3));
            }
            #[cfg(dev)]
            eprintln!("store: shutdown pool");
        }

        #[cfg(dev)]
        eprintln!("store: Store stopped");
    }

    /// dispatch an action
    ///
    /// ### Return
    /// * Ok(remains) : the number of remaining actions in the channel
    pub fn dispatch(&self, action: Action) -> Result<i64, StoreError> {
        let sender = self.dispatch_tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            let remains = tx.send(ActionOp::Action(action)).unwrap_or(0);
            if let Some(metrics) = &self.metrics {
                metrics.queue_size(remains as usize);
            }

            Ok(remains)
        } else {
            let err = StoreError::DispatchError("Dispatch channel is closed".to_string());
            if let Some(metrics) = &self.metrics {
                metrics.error_occurred(&err);
            }
            Err(err)
        }
    }

    /// Add middleware
    pub fn add_middleware(&self, middleware: Arc<Mutex<dyn Middleware<State, Action>>>) {
        self.middlewares.lock().unwrap().push(middleware);
    }
}

/// close tx channel when the store is dropped, but not the dispatcher
/// if you want to stop the dispatcher, call the stop method
impl<State, Action> Drop for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        self.close();
        // Shutdown the thread pool with timeout
        if let Ok(mut lk) = self.pool.lock() {
            if let Some(pool) = lk.take() {
                pool.shutdown_join_timeout(Duration::from_secs(3));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fmt::{Display, Formatter};
    use std::any::Any;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    struct TestReducer;
    //     Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync + 'static,
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            let new_state = state + action;
            DispatchOp::Dispatch(new_state, None)
        }
    }

    struct TestSubscriber;

    impl Subscriber<i32, i32> for TestSubscriber {
        fn on_notify(&self, state: &i32, action: &i32) {
            println!("on_notify: State {:}, Action: {:}", state, action);
        }
    }

    #[test]
    fn test_store_creation() {
        let reducer = Box::new(TestReducer);
        let store = Store::new(reducer);

        assert_eq!(store.get_state(), 0);
    }

    #[test]
    fn test_store_dispatch() {
        let reducer = Box::new(TestReducer);
        let store = Store::new(reducer);

        store.dispatch(1);
        store.stop();

        assert_eq!(store.get_state(), 1);
    }

    #[test]
    fn test_store_subscriber() {
        let reducer = Box::new(TestReducer);
        let store = Store::new(reducer);

        let subscriber = Arc::new(TestSubscriber);
        store.add_subscriber(subscriber);

        store.dispatch(1);
        store.stop();

        assert_eq!(store.get_state(), 1);
    }

    #[test]
    fn test_store_with_initial_state() {
        let reducer = Box::new(TestReducer);
        let store = Store::new_with_state(reducer, 10);

        assert_eq!(store.get_state(), 10);
    }

    #[test]
    fn test_store_with_name() {
        let reducer = Box::new(TestReducer);
        let store = Store::new_with_name(reducer, 10, "test_store".to_string()).unwrap();

        assert_eq!(store.get_state(), 10);
    }

    #[test]
    fn test_store_with_custom_capacity_and_policy() {
        let reducer = Box::new(TestReducer);
        let store = Store::new_with(
            vec![reducer],
            10,
            "test_store".to_string(),
            5,
            BackpressurePolicy::DropOldest,
            vec![],
            None,
        )
        .unwrap();

        assert_eq!(store.get_state(), 10);
    }

    struct PanicReducer;

    impl Reducer<i32, i32> for PanicReducer {
        #[allow(unreachable_code)]
        fn reduce(&self, _state: &i32, _action: &i32) -> DispatchOp<i32, i32> {
            panic!("reduce: panicking");
            DispatchOp::Dispatch(_state + _action, None)
        }
    }

    // no update state when a panic happened while reducing an action
    #[test]
    #[should_panic]
    fn test_panic_on_reducer() {
        // given
        let reducer: Box<PanicReducer> = Box::new(PanicReducer);
        let store = Store::new_with_state(reducer, 42);

        // when
        // if the panic occurs in a different thread created within the test function, the #[should_panic] attribute will not catch it
        // you can use the std::panic::catch_unwind function to catch the panic and then propagate it to the main thread.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.dispatch(1);
            store.stop();
        }));

        // then
        assert!(result.is_err());
        assert_eq!(store.get_state(), 42);
    }

    struct PanicSubscriber {
        state: Mutex<i32>,
    }
    impl Subscriber<i32, i32> for PanicSubscriber {
        #[allow(unreachable_code)]
        fn on_notify(&self, state: &i32, action: &i32) {
            panic!("on_notify: State: {}, Action: {}", *state, *action);
            *self.state.lock().unwrap() = *state;
        }
    }

    #[test]
    fn test_panic_on_subscriber() {
        // given
        let reducer = Box::new(TestReducer);
        let store = Store::new_with_state(reducer, 42);
        let panic_subscriber = Arc::new(PanicSubscriber {
            state: Mutex::new(0),
        });
        let panic_subscriber_clone = Arc::clone(&panic_subscriber);
        store.add_subscriber(panic_subscriber_clone);

        // when
        store.dispatch(1);
        store.stop();

        // then
        // the action reduced successfully,
        assert_eq!(store.get_state(), 43);
        // but the subscriber failed to get the state
        let state = panic_subscriber.state.lock().unwrap();
        assert_eq!(*state, 0);
    }

    #[derive(Debug, Clone)]
    enum EffectAction {
        ActionProduceEffect(i32),
        ResponseForTheEffect(i32),
    }

    struct EffectReducer {}
    impl EffectReducer {
        fn new() -> Self {
            Self {}
        }
    }
    impl Reducer<i32, EffectAction> for EffectReducer {
        fn reduce(&self, _state: &i32, action: &EffectAction) -> DispatchOp<i32, EffectAction> {
            println!("reduce: {:?}", _state);
            match action {
                EffectAction::ActionProduceEffect(value) => {
                    let new_state = *value;
                    // produce effect
                    let effect = Effect::Action(EffectAction::ResponseForTheEffect(new_state + 1));
                    DispatchOp::Dispatch(new_state, Some(effect))
                }
                EffectAction::ResponseForTheEffect(value) => {
                    let new_state = *value;
                    DispatchOp::Dispatch(new_state, None)
                }
            }
        }
    }

    #[test]
    fn test_effect() {
        let reducer = Box::new(EffectReducer::new());
        let store = Store::new(reducer);

        store.dispatch(EffectAction::ActionProduceEffect(42));

        // give time to the effect
        thread::sleep(Duration::from_secs(1));
        store.stop();

        assert_eq!(store.get_state(), 43);
    }

    struct TestMetrics {
        pub action_count: AtomicUsize,
        pub state_changes: AtomicUsize,
        pub notifications: AtomicUsize,
    }

    impl TestMetrics {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                action_count: AtomicUsize::new(0),
                state_changes: AtomicUsize::new(0),
                notifications: AtomicUsize::new(0),
            })
        }
    }

    impl Display for TestMetrics {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "action_count: {:?}",
                self.action_count.load(Ordering::SeqCst)
            )?;
            write!(
                f,
                ",state_changes: {:?}",
                self.state_changes.load(Ordering::SeqCst)
            )?;
            write!(
                f,
                ",notifications: {:?}",
                self.notifications.load(Ordering::SeqCst)
            )?;
            Ok(())
        }
    }

    impl StoreMetrics for TestMetrics {
        fn action_received(&self, _data: Option<&dyn Any>) {
            self.action_count.fetch_add(1, Ordering::SeqCst);
        }

        fn action_dropped(&self, _data: Option<&dyn Any>) {
            // Implementation for dropped actions
        }

        fn middleware_executed(
            &self,
            _data: Option<&dyn Any>,
            _middleware_name: &str,
            _count: usize,
            _duration: Duration,
        ) {
            // Implementation for middleware execution
        }

        fn action_reduced(&self, _data: Option<&dyn Any>, _duration: Duration) {
            self.state_changes.fetch_add(1, Ordering::SeqCst);
        }

        fn subscriber_notified(&self, _data: Option<&dyn Any>, _count: usize, _duration: Duration) {
            self.notifications.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_store_metrics() {
        let metrics = TestMetrics::new();
        let store = Store::new_with(
            vec![Box::new(TestReducer)],
            0,
            "test_store".to_string(),
            5,
            BackpressurePolicy::DropOldest,
            vec![],
            Some(metrics.clone()),
        )
        .unwrap();

        store.dispatch(1);
        store.stop();

        // Verify metrics were recorded
        assert!(metrics.action_count.load(Ordering::SeqCst) > 0);
        assert!(metrics.state_changes.load(Ordering::SeqCst) > 0);
        assert!(metrics.notifications.load(Ordering::SeqCst) > 0);
    }
}
