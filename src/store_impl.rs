use crate::channel::{BackpressureChannel, BackpressurePolicy, ReceiverChannel, SenderChannel};
use crate::dispatcher::Dispatcher;
use crate::metrics::{CountMetrics, Metrics, MetricsSnapshot};
use crate::middleware::Middleware;
use crate::{DispatchOp, Effect, MiddlewareOp, Reducer, Subscriber, Subscription};
use fmt::Debug;
use rusty_pool::ThreadPool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{fmt, thread};

use crate::iterator::{StateIterator, StateIteratorSubscriber};
use crate::store::{Store, StoreError, DEFAULT_CAPACITY, DEFAULT_STORE_NAME};

const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub enum ActionOp<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    Action(Action),
    AddSubscriber,
    #[allow(dead_code)]
    Exit(Instant),
}

/// StoreImpl is the default implementation of a Redux store
#[allow(clippy::type_complexity)]
pub struct StoreImpl<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    #[allow(dead_code)]
    pub(crate) name: String,
    state: Mutex<State>,
    pub(crate) reducers: Mutex<Vec<Box<dyn Reducer<State, Action> + Send + Sync>>>,
    pub(crate) subscribers: Arc<Mutex<Vec<Arc<dyn Subscriber<State, Action> + Send + Sync>>>>,
    /// 임시로 추가될 subscriber들을 저장하는 벡터
    adding_subscribers: Arc<Mutex<Vec<Arc<dyn Subscriber<State, Action> + Send + Sync>>>>,
    pub(crate) dispatch_tx: Mutex<Option<SenderChannel<Action>>>,
    middlewares: Mutex<Vec<Arc<dyn Middleware<State, Action> + Send + Sync>>>,
    pub(crate) metrics: Arc<CountMetrics>,
    /// thread pool for the store
    pub(crate) pool: Mutex<Option<ThreadPool>>,
}

/// Subscription for a subscriber
/// the subscriber can use it to unsubscribe from the store
struct SubscriberSubscription {
    unsubscribe: Box<dyn Fn() + Send + Sync>,
}

impl Subscription for SubscriberSubscription {
    fn unsubscribe(&self) {
        (self.unsubscribe)();
    }
}

impl<State, Action> StoreImpl<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    /// create a new store with an initial state
    pub fn new(state: State) -> Arc<StoreImpl<State, Action>> {
        Self::new_with(
            state,
            vec![],
            DEFAULT_STORE_NAME.into(),
            DEFAULT_CAPACITY,
            BackpressurePolicy::default(),
            vec![],
        )
        .unwrap()
    }

    /// create a new store with a reducer and an initial state
    pub fn new_with_reducer(
        state: State,
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
    ) -> Arc<StoreImpl<State, Action>> {
        Self::new_with_name(state, reducer, DEFAULT_STORE_NAME.into()).unwrap()
    }

    /// create a new store with name
    pub fn new_with_name(
        state: State,
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
        name: String,
    ) -> Result<Arc<StoreImpl<State, Action>>, StoreError> {
        Self::new_with(
            state,
            vec![reducer],
            name,
            DEFAULT_CAPACITY,
            BackpressurePolicy::default(),
            vec![],
        )
    }

    /// create a new store
    pub fn new_with(
        state: State,
        reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
        name: String,
        capacity: usize,
        policy: BackpressurePolicy<Action>,
        middlewares: Vec<Arc<dyn Middleware<State, Action> + Send + Sync>>,
    ) -> Result<Arc<StoreImpl<State, Action>>, StoreError> {
        let metrics = Arc::new(CountMetrics::default());
        let (tx, rx) = BackpressureChannel::<Action>::pair_with(
            "dispatch",
            capacity,
            policy.clone(),
            Some(metrics.clone()),
        );

        let store = StoreImpl {
            name: name.clone(),
            state: Mutex::new(state),
            reducers: Mutex::new(reducers),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            adding_subscribers: Arc::new(Mutex::new(Vec::default())),
            middlewares: Mutex::new(middlewares),
            dispatch_tx: Mutex::new(Some(tx)),
            metrics,
            pool: Mutex::new(Some(
                rusty_pool::Builder::new().name(format!("{}-pool", name)).build(),
            )),
        };

        // start a thread in which the store will listen for actions
        let rx_store = Arc::new(store);
        let tx_store = rx_store.clone();

        // reducer 스레드
        tx_store.pool.lock().unwrap().as_ref().unwrap().execute(move || {
            #[cfg(feature = "store-log")]
            eprintln!("store: reducer thread started");

            while let Some(action_op) = rx.recv() {
                let action_received_at = Instant::now();
                rx_store.metrics.action_received(Some(&action_op));

                match action_op {
                    ActionOp::Action(action) => {
                        let the_dispatcher = Arc::new(rx_store.clone());

                        // do reduce
                        let current_state = rx_store.state.lock().unwrap().clone();
                        let (need_dispatch, new_state, effects) = rx_store.do_reduce(
                            &action,
                            current_state,
                            the_dispatcher.clone(),
                            action_received_at,
                        );
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
                            rx_store.do_notify(
                                &action,
                                &new_state,
                                the_dispatcher.clone(),
                                action_received_at,
                            );
                        }

                        rx_store
                            .metrics
                            .action_executed(Some(&action), action_received_at.elapsed());
                    }
                    ActionOp::AddSubscriber => {
                        let current_state = rx_store.state.lock().unwrap().clone();
                        let mut adding_subscribers = rx_store.adding_subscribers.lock().unwrap();
                        let mut subscribers = rx_store.subscribers.lock().unwrap();

                        // 새로운 subscriber들에게 최신 상태를 전달하고 subscribers에 추가
                        for subscriber in adding_subscribers.drain(..) {
                            // 새로운 subscriber에게 최신 상태를 전달
                            subscriber.on_subscribe(&current_state);
                            subscribers.push(subscriber);
                        }

                        #[cfg(feature = "store-log")]
                        eprintln!("store: new subscribers added");
                    }
                    ActionOp::Exit(_) => {
                        rx_store.on_close(action_received_at);
                        #[cfg(feature = "store-log")]
                        eprintln!("store: reducer loop exit");
                        break;
                    }
                }
            }

            // drop all subscribers
            rx_store.clear_subscribers();

            #[cfg(feature = "store-log")]
            eprintln!("store: reducer thread done");
        });

        Ok(tx_store)
    }

    /// get the latest state(for debugging)
    ///
    /// prefer to use `subscribe` to get the state
    pub fn get_state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// get the metrics
    pub fn get_metrics(&self) -> MetricsSnapshot {
        (&(*self.metrics)).into()
    }

    /// add a reducer to the store
    pub fn add_reducer(&self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
        self.reducers.lock().unwrap().push(reducer);
    }

    /// add a subscriber to the store
    pub fn add_subscriber(
        &self,
        subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Box<dyn Subscription> {
        // 새로운 subscriber를 adding_subscribers에 추가
        self.adding_subscribers.lock().unwrap().push(subscriber.clone());

        // ActionOp::AddSubscriber 액션을 전달하여 reducer에서 처리하도록 함
        if let Some(tx) = self.dispatch_tx.lock().unwrap().as_ref() {
            let _ = tx.send(ActionOp::AddSubscriber);
        }

        // disposer for the subscriber
        let subscribers = self.subscribers.clone();
        let adding_subscribers = self.adding_subscribers.clone();
        Box::new(SubscriberSubscription {
            unsubscribe: Box::new(move || {
                let mut subscribers = subscribers.lock().unwrap();
                subscribers.retain(|s| {
                    let retain = !Arc::ptr_eq(s, &subscriber);
                    if !retain {
                        s.on_unsubscribe();
                    }
                    retain
                });

                // adding_subscribers에서도 제거
                let mut adding = adding_subscribers.lock().unwrap();
                adding.retain(|s| !Arc::ptr_eq(s, &subscriber));
            }),
        })
    }

    /// clear all subscribers
    pub(crate) fn clear_subscribers(&self) {
        #[cfg(feature = "store-log")]
        eprintln!("store: clear_subscribers");
        match self.subscribers.lock() {
            Ok(mut subscribers) => {
                for subscriber in subscribers.iter() {
                    subscriber.on_unsubscribe();
                }
                subscribers.clear();
            }
            Err(mut e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking subscribers: {:?}", e);
                for subscriber in e.get_ref().iter() {
                    subscriber.on_unsubscribe();
                }
                e.get_mut().clear();
            }
        }
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
        action_received_at: Instant,
    ) -> (bool, State, Option<Vec<Effect<Action>>>) {
        //let state = self.state.lock().unwrap().clone();

        let mut reduce_action = true;
        if !self.middlewares.lock().unwrap().is_empty() {
            let middleware_start = Instant::now();
            let mut middleware_executed = 0;
            for middleware in self.middlewares.lock().unwrap().iter() {
                middleware_executed += 1;
                match middleware.before_reduce(action, &state, dispatcher.clone()) {
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
                    Err(e) => {
                        middleware.on_error(e);
                    }
                }
            }
            let middleware_duration = middleware_start.elapsed();
            self.metrics.middleware_executed(
                Some(action),
                "before_reduce",
                middleware_executed,
                middleware_duration,
            );
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
            let reducer_duration = reducer_start.elapsed();
            self.metrics.action_reduced(
                Some(action),
                reducer_duration,
                action_received_at.elapsed(),
            );
        }

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
        self.metrics.effect_issued(effects.len());

        if !self.middlewares.lock().unwrap().is_empty() {
            let middleware_start = Instant::now();
            let mut middleware_executed = 0;
            for middleware in self.middlewares.lock().unwrap().iter() {
                middleware_executed += 1;
                match middleware.before_effect(action, state, effects, dispatcher.clone()) {
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
                    Err(e) => {
                        middleware.on_error(e);
                    }
                }
            }

            let middleware_duration = middleware_start.elapsed();
            self.metrics.middleware_executed(
                Some(action),
                "before_effect",
                middleware_executed,
                middleware_duration,
            );
        }

        let effects_total = effects.len();
        while !effects.is_empty() {
            let effect = effects.remove(0);
            match effect {
                Effect::Action(a) => {
                    dispatcher.dispatch_thunk(Box::new(move |dispatcher| {
                        dispatcher.dispatch(a).expect("no dispatch failed");
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

        let duration = effect_start.elapsed();
        self.metrics.effect_executed(effects_total, duration);
    }

    pub(crate) fn do_notify(
        &self,
        action: &Action,
        next_state: &State,
        dispatcher: Arc<dyn Dispatcher<Action>>,
        _action_received_at: Instant,
    ) {
        let _notify_start = Instant::now();
        self.metrics.state_notified(Some(next_state));

        let mut need_notify = true;
        if !self.middlewares.lock().unwrap().is_empty() {
            let middleware_start = Instant::now();
            let mut middleware_executed = 0;
            for middleware in self.middlewares.lock().unwrap().iter() {
                middleware_executed += 1;
                match middleware.before_dispatch(action, next_state, dispatcher.clone()) {
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
                    Err(e) => {
                        middleware.on_error(e);
                    }
                }
            }
            let middleware_duration = middleware_start.elapsed();
            self.metrics.middleware_executed(
                Some(action),
                "before_dispatch",
                middleware_executed,
                middleware_duration,
            );
        }

        if need_notify {
            let subscribers = self.subscribers.lock().unwrap().clone();
            for subscriber in subscribers.iter() {
                subscriber.on_notify(next_state, action);
            }
            let duration = _notify_start.elapsed();
            self.metrics.subscriber_notified(Some(action), subscribers.len(), duration);
        }
    }

    fn on_close(&self, action_received_at: Instant) {
        #[cfg(feature = "store-log")]
        eprintln!("store: on_close");

        self.metrics.action_executed(None, action_received_at.elapsed());
    }

    /// close the store
    ///
    /// send an exit action to the store and drop the dispatch channel
    pub fn close(&self) {
        match self.dispatch_tx.lock() {
            Ok(mut tx) => {
                if let Some(tx) = tx.take() {
                    #[cfg(feature = "store-log")]
                    eprintln!("store: closing dispatch channel");
                    match tx.send(ActionOp::Exit(Instant::now())) {
                        Ok(_) => {
                            #[cfg(feature = "store-log")]
                            eprintln!("store: dispatch channel sent exit");
                        }
                        Err(_e) => {
                            #[cfg(feature = "store-log")]
                            eprintln!("store: Error while closing dispatch channel");
                        }
                    }
                    drop(tx);
                }
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking dispatch channel: {:?}", _e);
                return;
            }
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: dispatch channel closed");
    }

    /// close the store and wait for the dispatcher to finish
    pub fn stop(&self) {
        self.close();

        // Shutdown the thread pool with timeout
        // lock pool
        match self.pool.lock() {
            Ok(mut pool) => {
                if let Some(pool) = pool.take() {
                    pool.shutdown_join();
                }
                #[cfg(feature = "store-log")]
                eprintln!("store: shutdown pool");
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking pool: {:?}", _e);
                return;
            }
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: Store stopped");
    }

    /// close the store and wait for the dispatcher to finish
    pub fn stop_with_timeout(&self, timeout: Duration) {
        self.close();

        // Shutdown the thread pool with timeout
        // lock pool
        match self.pool.lock() {
            Ok(mut pool) => {
                if let Some(pool) = pool.take() {
                    pool.shutdown_join_timeout(timeout);
                }
                #[cfg(feature = "store-log")]
                eprintln!("store: shutdown pool");
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking pool: {:?}", _e);
                return;
            }
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: Store stopped");
    }

    /// dispatch an action
    ///
    /// ### Return
    /// * Ok(()) : if the action is dispatched
    /// * Err(StoreError) : if the dispatch channel is closed
    pub fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        let sender = self.dispatch_tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            // the number of remaining actions in the channel
            let remains = tx.send(ActionOp::Action(action)).unwrap_or(0);
            self.metrics.queue_size(remains as usize);
            Ok(())
        } else {
            let err = StoreError::DispatchError("Dispatch channel is closed".to_string());
            self.metrics.error_occurred(&err);
            Err(err)
        }
    }

    /// Add middleware
    pub fn add_middleware(&self, middleware: Arc<dyn Middleware<State, Action> + Send + Sync>) {
        self.middlewares.lock().unwrap().push(middleware.clone());
    }

    /// Iterator for the state
    ///
    /// it uses a channel to subscribe to the state changes
    /// the channel is rendezvous(capacity 1), the store will block on the channel until the subscriber consumes the state
    pub fn iter(&self) -> impl Iterator<Item = (State, Action)> {
        self.iter_with(1, BackpressurePolicy::BlockOnFull)
    }

    /// Iterator for the state
    ///  
    /// ### Parameters
    /// * capacity: the capacity of the channel
    /// * policy: the backpressure policy
    pub(crate) fn iter_with(
        &self,
        capacity: usize,
        policy: BackpressurePolicy<(State, Action)>,
    ) -> impl Iterator<Item = (State, Action)> {
        let (iter_tx, iter_rx) = BackpressureChannel::<(State, Action)>::pair_with(
            "store_iter",
            capacity,
            policy,
            Some(self.metrics.clone()),
        );

        let subscription = self.add_subscriber(Arc::new(StateIteratorSubscriber::new(iter_tx)));
        StateIterator::new(iter_rx, subscription)
    }

    /// subscribing to store updates in new context
    /// with default capacity and `BlockOnFull` policy when the channel is full
    ///
    /// ## Parameters
    /// * subscriber: The subscriber to subscribe to the store
    ///
    /// ## Return
    /// * Subscription: Subscription for the store,
    pub fn subscribed(
        &self,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError> {
        self.subscribed_with(
            DEFAULT_CAPACITY,
            BackpressurePolicy::BlockOnFull,
            subscriber,
        )
    }

    /// subscribing to store updates in new context
    ///
    /// ### Parameters
    /// * capacity: Channel buffer capacity
    /// * policy: Backpressure policy for when channel is full,
    ///     `BlockOnFull` or `DropLatestIf` is supported to prevent from dropping the ActionOp::Exit
    ///
    /// ### Return
    /// * Subscription: Subscription for the store,
    pub fn subscribed_with(
        &self,
        capacity: usize,
        policy: BackpressurePolicy<(Instant, State, Action)>,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError> {
        // spsc channel
        let (tx, rx) = BackpressureChannel::<(Instant, State, Action)>::pair_with(
            format!("{}-channel", self.name).as_str(),
            capacity,
            policy,
            Some(self.metrics.clone()),
        );

        // channeled thread
        let thread_name = format!("{}-channeled-subscriber", self.name);
        let metrics_clone = self.metrics.clone();
        let builder = thread::Builder::new().name(thread_name.clone());
        let handle = match builder.spawn(move || {
            // subscribe to the store
            Self::subscribed_loop(thread_name, rx, subscriber, metrics_clone);
        }) {
            Ok(h) => h,
            Err(e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while spawning channel thread: {:?}", e);
                return Err(StoreError::SubscriptionError(format!(
                    "Error while spawning channel thread: {:?}",
                    e
                )));
            }
        };

        // subscribe to the store
        let channel_subscriber = Arc::new(ChanneledSubscriber::new(handle, tx));
        let subscription = self.add_subscriber(channel_subscriber.clone());

        Ok(subscription)
    }

    fn subscribed_loop(
        _name: String,
        rx: ReceiverChannel<(Instant, State, Action)>,
        subscriber: Box<dyn Subscriber<State, Action>>,
        metrics: Arc<dyn Metrics>,
    ) {
        #[cfg(feature = "store-log")]
        eprintln!("store: {} channel thread started", _name);

        while let Some(msg) = rx.recv() {
            match msg {
                ActionOp::Action((created_at, state, action)) => {
                    let started_at = Instant::now();
                    {
                        subscriber.on_notify(&state, &action);
                    }
                    metrics.subscriber_notified(Some(&action), 1, started_at.elapsed());

                    // action executed
                    metrics.action_executed(Some(&action), created_at.elapsed());
                }
                ActionOp::AddSubscriber => {
                    // AddSubscriber는 채널된 subscriber에서는 처리하지 않음
                    // 이는 메인 reducer 스레드에서만 처리됨
                    #[cfg(feature = "store-log")]
                    eprintln!("store: {} received AddSubscriber (ignored)", _name);
                }
                ActionOp::Exit(created_at) => {
                    metrics.action_executed(None, created_at.elapsed());
                    #[cfg(feature = "store-log")]
                    eprintln!("store: {} channel thread loop exit", _name);
                    break;
                }
            }
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: {} channel thread done", _name);
    }
}

/// Subscriber implementation that forwards store updates to a channel
struct ChanneledSubscriber<T>
where
    T: Send + Sync + Clone + 'static,
{
    handle: Mutex<Option<JoinHandle<()>>>,
    tx: Mutex<Option<SenderChannel<T>>>,
}

impl<T> ChanneledSubscriber<T>
where
    T: Send + Sync + Clone + 'static,
{
    pub(crate) fn new(handle: JoinHandle<()>, tx: SenderChannel<T>) -> Self {
        Self {
            handle: Mutex::new(Some(handle)),
            tx: Mutex::new(Some(tx)),
        }
    }

    fn clear_resource(&self) {
        // drop channel
        if let Ok(mut tx_locked) = self.tx.lock() {
            if let Some(tx) = tx_locked.take() {
                let _ = tx.send(ActionOp::Exit(Instant::now()));
                drop(tx);
            };
        }

        // join the thread
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
    }
}

impl<State, Action> Subscriber<State, Action> for ChanneledSubscriber<(Instant, State, Action)>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn on_notify(&self, state: &State, action: &Action) {
        match self.tx.lock() {
            Ok(tx) => {
                tx.as_ref().map(|tx| {
                    tx.send(ActionOp::Action((
                        Instant::now(),
                        state.clone(),
                        action.clone(),
                    )))
                });
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking channel: {:?}", _e);
            }
        }
    }

    fn on_unsubscribe(&self) {
        self.clear_resource();
    }
}

impl<T> Subscription for ChanneledSubscriber<T>
where
    T: Send + Sync + Clone + 'static,
{
    fn unsubscribe(&self) {
        self.clear_resource();
    }
}

/// close tx channel when the store is dropped, but not the dispatcher
/// if you want to stop the dispatcher, call the stop method
impl<State, Action> Drop for StoreImpl<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        self.close();

        // Shutdown the thread pool with timeout
        self.stop_with_timeout(DEFAULT_STOP_TIMEOUT);
        // if let Ok(mut lk) = self.pool.lock() {
        //     if let Some(pool) = lk.take() {
        //         pool.shutdown_join_timeout(Duration::from_secs(3));
        //     }
        // }

        #[cfg(feature = "store-log")]
        eprintln!("store: '{}' Store dropped", self.name);
    }
}

impl<State, Action> Store<State, Action> for StoreImpl<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn get_state(&self) -> State {
        self.get_state()
    }

    fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        self.dispatch(action)
    }

    fn add_subscriber(
        &self,
        subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Box<dyn Subscription> {
        self.add_subscriber(subscriber)
    }

    fn subscribed(
        &self,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError> {
        self.subscribed(subscriber)
    }

    fn subscribed_with(
        &self,
        capacity: usize,
        policy: BackpressurePolicy<(Instant, State, Action)>,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError> {
        self.subscribed_with(capacity, policy, subscriber)
    }

    fn stop(&self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    struct TestChannelSubscriber {
        received: Arc<Mutex<Vec<(i32, i32)>>>,
    }

    impl TestChannelSubscriber {
        fn new(received: Arc<Mutex<Vec<(i32, i32)>>>) -> Self {
            Self { received }
        }
    }

    impl Subscriber<i32, i32> for TestChannelSubscriber {
        fn on_notify(&self, state: &i32, action: &i32) {
            println!("TestChannelSubscriber: state={}, action={}", state, action);
            self.received.lock().unwrap().push((*state, *action));
        }
    }

    struct TestReducer;

    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            DispatchOp::Dispatch(state + action, None)
        }
    }

    struct SlowSubscriber {
        received: Arc<Mutex<Vec<(i32, i32)>>>,
        delay: Duration,
    }

    impl SlowSubscriber {
        fn new(received: Arc<Mutex<Vec<(i32, i32)>>>, delay: Duration) -> Self {
            Self { received, delay }
        }
    }

    impl Subscriber<i32, i32> for SlowSubscriber {
        fn on_notify(&self, state: &i32, action: &i32) {
            println!("SlowSubscriber: state={}, action={}", state, action);
            std::thread::sleep(self.delay);
            self.received.lock().unwrap().push((*state, *action));
        }
    }

    #[test]
    fn test_store_subscribed_basic() {
        // Setup store with a simple counter
        let initial_state = 0;
        let reducer = Box::new(TestReducer);
        let store = StoreImpl::new_with_reducer(initial_state, reducer);

        // Create subscriber to receive updates
        let received_states = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Box::new(TestChannelSubscriber::new(received_states.clone()));
        // Create channel
        let subscription = store.subscribed_with(10, BackpressurePolicy::DropOldest, subscriber1);

        // Dispatch some actions
        store.dispatch(1).unwrap();
        store.dispatch(2).unwrap();

        // Give some time for processing
        // thread::sleep(Duration::from_millis(100));
        store.stop();

        // unsubscribe from the channel
        subscription.unwrap().unsubscribe();

        // Verify received updates
        let states = received_states.lock().unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0], (1, 1)); // (state, action)
        assert_eq!(states[1], (3, 2)); // state=1+2, action=2
    }

    #[test]
    fn test_store_subscribed_backpressure() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let subscriber = Box::new(SlowSubscriber::new(
            received_clone,
            Duration::from_millis(100),
        ));
        // Create channel with small capacity
        let subscription = store.subscribed_with(1, BackpressurePolicy::DropOldest, subscriber);

        // Fill the channel
        for i in 0..5 {
            store.dispatch(i).unwrap();
        }

        // Give some time for having channel thread to process
        thread::sleep(Duration::from_millis(200));
        store.stop();
        subscription.unwrap().unsubscribe();

        // Should only receive the latest updates due to backpressure
        let received = received.lock().unwrap();
        assert!(received.len() <= 2); // Some messages should be dropped

        if let Some((state, action)) = received.last() {
            assert_eq!(*action, 4); // Last action should be received
            assert!(*state <= 10); // Final state should be sum of 0..5
        }
    }

    #[test]
    fn test_store_subscribed_subscription() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Box::new(TestChannelSubscriber::new(received.clone()));
        let subscription = store.subscribed_with(10, BackpressurePolicy::DropOldest, subscriber1);

        // Dispatch some actions
        store.dispatch(1).unwrap();

        // give some time for processing
        thread::sleep(Duration::from_millis(100));
        // subscriber should receive the state
        assert_eq!(received.lock().unwrap().len(), 1);

        // unsubscribe
        subscription.unwrap().unsubscribe();

        // dispatch more actions
        store.dispatch(2).unwrap();
        store.dispatch(3).unwrap();
        // give some time for processing
        store.stop();
        // subscriber should not receive the state
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    // 새로운 subscriber가 추가될 때 최신 상태를 받는지 테스트
    #[test]
    fn test_new_subscriber_receives_latest_state() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // 첫 번째 subscriber 추가
        let received1 = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Arc::new(TestChannelSubscriber::new(received1.clone()));
        store.add_subscriber(subscriber1);

        // 액션을 dispatch하여 상태 변경
        store.dispatch(5).unwrap();
        store.dispatch(10).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 두 번째 subscriber 추가 (현재 상태는 15)
        let received2 = Arc::new(Mutex::new(Vec::new()));
        let subscriber2 = Arc::new(TestChannelSubscriber::new(received2.clone()));
        store.add_subscriber(subscriber2);

        // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 새로운 액션을 dispatch
        store.dispatch(20).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 첫 번째 subscriber는 모든 상태 변경을 받아야 함
        let received1 = received1.lock().unwrap();
        assert_eq!(received1.len(), 3);
        assert_eq!(received1[0], (5, 5));
        assert_eq!(received1[1], (15, 10));
        assert_eq!(received1[2], (35, 20));

        // 두 번째 subscriber는 추가된 후의 상태 변경만 받아야 함
        let received2 = received2.lock().unwrap();
        assert_eq!(received2.len(), 1);
        assert_eq!(received2[0], (35, 20));
    }

    // 새로운 subscriber가 추가될 때 on_subscribe가 호출되는지 테스트
    #[test]
    fn test_new_subscriber_on_subscribe_called() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // 액션을 dispatch하여 상태 변경
        store.dispatch(5).unwrap();

        // on_subscribe를 구현한 subscriber 추가
        let received_states = Arc::new(Mutex::new(Vec::new()));
        let subscribe_called = Arc::new(Mutex::new(false));

        struct TestSubscribeSubscriber {
            received_states: Arc<Mutex<Vec<i32>>>,
            subscribe_called: Arc<Mutex<bool>>,
        }

        impl Subscriber<i32, i32> for TestSubscribeSubscriber {
            fn on_subscribe(&self, state: &i32) {
                self.received_states.lock().unwrap().push(*state);
                *self.subscribe_called.lock().unwrap() = true;
            }

            fn on_notify(&self, state: &i32, _action: &i32) {
                self.received_states.lock().unwrap().push(*state);
            }
        }

        let subscriber = Arc::new(TestSubscribeSubscriber {
            received_states: received_states.clone(),
            subscribe_called: subscribe_called.clone(),
        });

        store.add_subscriber(subscriber);

        // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // on_subscribe가 호출되었는지 확인
        assert!(*subscribe_called.lock().unwrap());

        // 최신 상태(5)를 받았는지 확인
        let states = received_states.lock().unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0], 5);
    }

    // 여러 subscriber가 동시에 추가될 때 테스트
    #[test]
    fn test_multiple_subscribers_added_simultaneously() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // 액션을 dispatch하여 상태 변경
        store.dispatch(10).unwrap();
        store.dispatch(20).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 여러 subscriber를 동시에 추가
        let subscribers = vec![
            Arc::new(TestChannelSubscriber::new(Arc::new(Mutex::new(Vec::new())))),
            Arc::new(TestChannelSubscriber::new(Arc::new(Mutex::new(Vec::new())))),
            Arc::new(TestChannelSubscriber::new(Arc::new(Mutex::new(Vec::new())))),
        ];

        for subscriber in &subscribers {
            store.add_subscriber(subscriber.clone());
        }

        // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 새로운 액션을 dispatch
        store.dispatch(30).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 모든 subscriber가 새로운 액션을 받았는지 확인
        for subscriber in &subscribers {
            let received = subscriber.received.lock().unwrap();
            assert_eq!(received.len(), 1);
            assert_eq!(received[0], (60, 30)); // state: 30+30, action: 30
        }
    }

    // subscriber 추가 후 즉시 unsubscribe하는 테스트
    #[test]
    fn test_subscriber_unsubscribe_after_add() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // 액션을 dispatch하여 상태 변경
        store.dispatch(5).unwrap();

        // subscriber 추가
        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Arc::new(TestChannelSubscriber::new(received.clone()));
        let subscription = store.add_subscriber(subscriber);

        // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 즉시 unsubscribe
        subscription.unsubscribe();

        // 새로운 액션을 dispatch
        store.dispatch(10).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // subscriber가 새로운 액션을 받지 않았는지 확인
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 0);
    }

    // store가 중지된 후 subscriber를 추가하는 테스트
    #[test]
    fn test_add_subscriber_after_store_stop() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // store 중지
        store.stop();

        // subscriber 추가 시도
        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Arc::new(TestChannelSubscriber::new(received.clone()));
        let _subscription = store.add_subscriber(subscriber);

        // 잠시 대기
        thread::sleep(Duration::from_millis(100));

        // subscriber가 추가되었지만 store가 중지되어 있으므로 액션을 받지 않음
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 0);
    }

    // on_subscribe에서 상태를 수정하는 subscriber 테스트
    #[test]
    fn test_subscriber_modifies_state_in_on_subscribe() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // 액션을 dispatch하여 상태 변경
        store.dispatch(5).unwrap();

        struct ModifyingSubscriber {
            received_states: Arc<Mutex<Vec<i32>>>,
            subscribe_called: Arc<Mutex<bool>>,
        }

        impl Subscriber<i32, i32> for ModifyingSubscriber {
            fn on_subscribe(&self, state: &i32) {
                // on_subscribe에서 상태를 수정해도 store의 상태는 변경되지 않음
                self.received_states.lock().unwrap().push(*state);
                *self.subscribe_called.lock().unwrap() = true;
            }

            fn on_notify(&self, state: &i32, _action: &i32) {
                self.received_states.lock().unwrap().push(*state);
            }
        }

        let subscriber = Arc::new(ModifyingSubscriber {
            received_states: Arc::new(Mutex::new(Vec::new())),
            subscribe_called: Arc::new(Mutex::new(false)),
        });

        store.add_subscriber(subscriber.clone());

        // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // on_subscribe가 호출되었는지 확인
        assert!(*subscriber.subscribe_called.lock().unwrap());

        // 최신 상태(5)를 받았는지 확인
        let states = subscriber.received_states.lock().unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0], 5);

        // store의 상태가 변경되지 않았는지 확인
        assert_eq!(store.get_state(), 5);
    }

    // 여러 번의 AddSubscriber 액션이 연속으로 발생하는 테스트
    #[test]
    fn test_consecutive_add_subscriber_actions() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer));

        // 첫 번째 subscriber 추가
        let received1 = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Arc::new(TestChannelSubscriber::new(received1.clone()));
        store.add_subscriber(subscriber1);

        // 잠시 대기
        thread::sleep(Duration::from_millis(50));

        // 두 번째 subscriber 추가
        let received2 = Arc::new(Mutex::new(Vec::new()));
        let subscriber2 = Arc::new(TestChannelSubscriber::new(received2.clone()));
        store.add_subscriber(subscriber2);

        // 잠시 대기
        thread::sleep(Duration::from_millis(50));

        // 세 번째 subscriber 추가
        let received3 = Arc::new(Mutex::new(Vec::new()));
        let subscriber3 = Arc::new(TestChannelSubscriber::new(received3.clone()));
        store.add_subscriber(subscriber3);

        // 잠시 대기하여 모든 AddSubscriber 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 새로운 액션을 dispatch
        store.dispatch(10).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 모든 subscriber가 새로운 액션을 받았는지 확인
        assert_eq!(received1.lock().unwrap().len(), 1);
        assert_eq!(received2.lock().unwrap().len(), 1);
        assert_eq!(received3.lock().unwrap().len(), 1);
    }
}
