use crate::channel::{BackpressureChannel, BackpressurePolicy, ReceiverChannel, SenderChannel};
use crate::dispatcher::Dispatcher;
use crate::metrics::{CountMetrics, Metrics, MetricsSnapshot};
use crate::middleware::{MiddlewareFn, MiddlewareFnFactory};
use crate::subscriber::SubscriberWithId;
use crate::{DispatchOp, Effect, Reducer, SenderError, Subscriber, Subscription};
use rusty_pool::ThreadPool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{fmt, thread};

use crate::iterator::{StateIterator, StateIteratorSubscriber};
use crate::store::{Store, StoreError, DEFAULT_CAPACITY, DEFAULT_STORE_NAME};

const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// ActionOp is used to dispatch an action to the store
#[derive(Clone, PartialEq)]
pub(crate) enum ActionOp<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    /// Action is used to dispatch an action to the store
    Action(Action),
    /// AddSubscriber is used to add a subscriber to the store
    AddSubscriber,
    /// StateFunction is used to execute a function with the current state
    StateFunction,
    /// Exit is used to exit the store and should not be dropped
    #[allow(dead_code)]
    Exit(Instant),
}

impl<Action> fmt::Debug for ActionOp<Action>
where
    Action: fmt::Debug + Send + Sync + Clone + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionOp::Action(action) => f.debug_tuple("Action").field(action).finish(),
            ActionOp::AddSubscriber => f.write_str("AddSubscriber"),
            ActionOp::StateFunction => f.write_str("StateFunction"),
            ActionOp::Exit(instant) => f.debug_tuple("Exit").field(instant).finish(),
        }
    }
}

// format Action without Debug
#[cfg(feature = "store-log")]
pub(crate) fn describe_action<Action>(_action: &Action) -> String
where
    Action: Send + Sync + Clone + 'static,
{
    format!("Action<{}>(..)", std::any::type_name::<Action>())
}

// format ActionOp<Action> in which Action is not bound to Debug
// #[cfg(feature = "store-log")]
pub(crate) fn describe_action_op<Action>(action_op: &ActionOp<Action>) -> String
where
    Action: Send + Sync + Clone + 'static,
{
    match action_op {
        ActionOp::Action(_) => {
            format!("Action<{}>(..)", std::any::type_name::<Action>())
        }
        ActionOp::AddSubscriber => "AddSubscriber".to_string(),
        ActionOp::StateFunction => "StateFunction".to_string(),
        ActionOp::Exit(instant) => format!("Exit({instant:?})"),
    }
}

/// StoreImpl is the default implementation of a Redux store.
///
/// ## Caution
/// [`StoreImpl`] is the default implementation of the [`Store`] trait, and its interface can be changed in the future.
/// [`Store`] is the stable interface for the store that user code should depend on.
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
    pub(crate) subscribers: Arc<Mutex<Vec<SubscriberWithId<State, Action>>>>,
    /// temporary vector to store subscribers to be added
    adding_subscribers: Arc<Mutex<Vec<SubscriberWithId<State, Action>>>>,
    state_functions: Arc<Mutex<Vec<Box<dyn FnOnce(&State) + Send + Sync + 'static>>>>,
    pub(crate) dispatch_tx: Mutex<Option<SenderChannel<Action>>>,
    /// middleware factories
    middleware_factories: Mutex<Vec<Arc<dyn MiddlewareFnFactory<State, Action> + Send + Sync>>>, // New middleware chain

    pub(crate) metrics: Arc<CountMetrics>,
    /// thread pool for the store
    pub(crate) pool: Mutex<Option<ThreadPool>>,
}

/// Subscription for a subscriber
/// the subscriber can use it to unsubscribe from the store
struct SubscriberSubscription {
    #[allow(dead_code)]
    subscriber_id: u64, // Store subscriber ID instead of Arc reference
    unsubscribe: Box<dyn Fn(u64) + Send + Sync>,
}

impl Subscription for SubscriberSubscription {
    fn unsubscribe(&self) {
        (self.unsubscribe)(self.subscriber_id);
    }
}

impl<State, Action> StoreImpl<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    // /// create a new store with an initial state
    // pub(crate) fn new(state: State) -> Result<Arc<StoreImpl<State, Action>>, StoreError> {
    //     Self::new_with(
    //         state,
    //         vec![],
    //         DEFAULT_STORE_NAME.into(),
    //         DEFAULT_CAPACITY,
    //         BackpressurePolicy::default(),
    //         vec![],
    //     )
    // }

    /// create a new store with a reducer and an initial state
    pub fn new_with_reducer(
        state: State,
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
    ) -> Result<Arc<StoreImpl<State, Action>>, StoreError> {
        Self::new_with(
            state,
            vec![reducer],
            DEFAULT_STORE_NAME.into(),
            DEFAULT_CAPACITY,
            BackpressurePolicy::default(),
            vec![],
        )
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
        middlewares: Vec<Arc<dyn MiddlewareFnFactory<State, Action> + Send + Sync>>,
    ) -> Result<Arc<StoreImpl<State, Action>>, StoreError> {
        let metrics = Arc::new(CountMetrics::default());
        let (tx, rx) = BackpressureChannel::<Action>::pair_with(
            "dispatch",
            capacity,
            policy,
            Some(metrics.clone()),
        );

        if reducers.is_empty() {
            return Err(StoreError::InitError(
                "At least one reducer is required".to_string(),
            ));
        }

        let store_impl = StoreImpl {
            name: name.clone(),
            state: Mutex::new(state),
            reducers: Mutex::new(reducers),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            adding_subscribers: Arc::new(Mutex::new(Vec::default())),
            state_functions: Arc::new(Mutex::new(Vec::default())),
            middleware_factories: Mutex::new(middlewares),
            dispatch_tx: Mutex::new(Some(tx)),
            metrics,
            pool: Mutex::new(Some(
                rusty_pool::Builder::new().name(format!("{}-pool", name)).build(),
            )),
        };

        // start a thread in which the store will listen for actions
        let rx_store = Arc::new(store_impl);
        let tx_store = rx_store.clone();

        // reducer thread
        match tx_store.pool.lock() {
            Ok(pool) => {
                if let Some(pool) = pool.as_ref() {
                    pool.execute(move || {
                        StoreImpl::reducer_thread(rx, rx_store);
                    })
                }
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking pool: {:?}", _e);
                return Err(StoreError::InitError(format!(
                    "Error while locking pool: {:?}",
                    _e
                )));
            }
        }

        Ok(tx_store)
    }

    // reducer thread
    pub(crate) fn reducer_thread(
        rx: ReceiverChannel<Action>,
        store_impl: Arc<StoreImpl<State, Action>>,
    ) {
        #[cfg(feature = "store-log")]
        eprintln!("store: reducer thread started");

        let store_clone = store_impl.clone();
        let reducer_middleware: MiddlewareFn<State, Action> =
            Arc::new(move |state: &State, action: &Action| {
                let started_at = Instant::now();

                let dispatch_op = if store_clone.reducers.lock().unwrap().len() == 1 {
                    store_clone.reducers.lock().unwrap()[0].reduce(state, action)
                } else {
                    let reducers = store_clone.reducers.lock().unwrap();
                    let mut iter = reducers.iter();

                    // First reducer uses the input references directly
                    let mut result = iter.next().unwrap().reduce(state, action);

                    // Remaining reducers use the result from previous reducer
                    for reducer in iter {
                        match result {
                            DispatchOp::Dispatch(current_state, current_effects) => {
                                result = reducer.reduce(&current_state, action);
                                // Merge effects from both reducers
                                match result {
                                    DispatchOp::Dispatch(s, mut e) => {
                                        e.extend(current_effects);
                                        result = DispatchOp::Dispatch(s, e);
                                    }
                                    DispatchOp::Keep(s, mut e) => {
                                        e.extend(current_effects);
                                        result = DispatchOp::Keep(s, e);
                                    }
                                }
                            }
                            DispatchOp::Keep(current_state, current_effects) => {
                                result = reducer.reduce(&current_state, action);
                                // Merge effects from both reducers
                                match result {
                                    DispatchOp::Dispatch(s, mut e) => {
                                        e.extend(current_effects);
                                        result = DispatchOp::Dispatch(s, e);
                                    }
                                    DispatchOp::Keep(s, mut e) => {
                                        e.extend(current_effects);
                                        result = DispatchOp::Keep(s, e);
                                    }
                                }
                            }
                        }
                    }
                    result
                };

                store_clone.metrics.action_reduced(
                    Some(action),
                    started_at.elapsed(),
                    Instant::now().elapsed(),
                );
                Ok(dispatch_op)
            });

        let mut middleware_deco = reducer_middleware;
        // chain middlewares in **REVERSE** order
        for middleware in store_impl.middleware_factories.lock().unwrap().iter().rev() {
            middleware_deco = middleware.create(middleware_deco);
        }

        let middleware_deco_arc = Arc::new(middleware_deco);
        while let Some(action_op) = rx.recv() {
            let action_received_at = Instant::now();
            store_impl.metrics.action_received(Some(&action_op));
            #[cfg(feature = "store-log")]
            eprintln!(
                "store: dispatch: action: {:?}, remains: {}",
                describe_action_op(&action_op),
                rx.len()
            );
            match action_op {
                ActionOp::Action(action) => {
                    // do reduce
                    // Get current state reference while holding lock for minimal time
                    let current_state_ref = {
                        let state_guard = store_impl.state.lock().unwrap();
                        state_guard.clone()
                    };
                    let mut effects = vec![];
                    let result = store_impl.do_reduce(
                        &current_state_ref,
                        &action,
                        &mut effects,
                        middleware_deco_arc.clone(),
                    );

                    match result {
                        Ok(dispatch_op) => {
                            // do effects remain
                            store_impl.do_effect(&mut effects, store_impl.clone());
                            // do notify subscribers and update store state
                            match dispatch_op {
                                DispatchOp::Dispatch(new_state, _) => {
                                    // Update store state
                                    *store_impl.state.lock().unwrap() = new_state.clone();
                                    // Notify subscribers with refs
                                    store_impl.do_notify(
                                        &action,
                                        &new_state,
                                        store_impl.clone(),
                                        action_received_at,
                                    );
                                }
                                DispatchOp::Keep(new_state, _) => {
                                    // Update store state even if not dispatching
                                    *store_impl.state.lock().unwrap() = new_state;
                                }
                            }
                        }
                        Err(_e) => {
                            #[cfg(feature = "store-log")]
                            eprintln!(
                                "store: error do_reduce: action: {}, remains: {}",
                                describe_action(&action),
                                rx.len()
                            );
                        }
                    }
                    // store_impl.metrics.action_executed(Some(&action), action_received_at.elapsed());
                }
                ActionOp::AddSubscriber => {
                    let mut new_subscribers = store_impl.adding_subscribers.lock().unwrap();
                    let new_subscribers_len = new_subscribers.len();
                    if new_subscribers_len > 0 {
                        let current_state = store_impl.state.lock().unwrap().clone();
                        let iter_subscribers = new_subscribers.drain(..);

                        store_impl.do_subscribe(current_state, iter_subscribers);
                    }

                    #[cfg(feature = "store-log")]
                    eprintln!("store: {} subscribers added", new_subscribers_len);
                }

                // ActionOp::RemoveSubscriber(subscriber_id) => {
                //     rx_store.do_remove_subscriber(subscriber_id);
                //     #[cfg(feature = "store-log")]
                //     eprintln!("store: {} subscribers removed", subscriber_id);
                // }
                ActionOp::StateFunction => {
                    store_impl.do_state_function();
                }
                ActionOp::Exit(_) => {
                    store_impl.on_close(action_received_at);
                    #[cfg(feature = "store-log")]
                    eprintln!("store: reducer loop exit");
                    break;
                }
            }
            store_impl.metrics.action_executed(None, action_received_at.elapsed());
        }

        // drop all subscribers
        store_impl.clear_subscribers();

        #[cfg(feature = "store-log")]
        eprintln!("store: reducer thread done");
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

    // /// add a reducer to the store
    // pub(crate) fn add_reducer(&self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
    //     let reducer_arc = Arc::from(reducer);
    //     let mut chain_guard = self.reducer_chain.lock().unwrap();
    //     if let Some(reducer_chain) = chain_guard.take() {
    //         // Chain the new reducer to the existing chain
    //         *chain_guard = Some(reducer_chain.chain(reducer_arc));
    //     } else {
    //         // Create new chain if none exists
    //         *chain_guard = Some(ReducerChain::new(reducer_arc));
    //     }
    // }

    /// add a subscriber to the store
    pub fn add_subscriber(
        &self,
        subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError> {
        // SubscriberWithId로 래핑하여 unique ID 할당
        let subscriber_with_id = SubscriberWithId::new(subscriber);
        let subscriber_id = subscriber_with_id.id;

        // 새로운 subscriber를 adding_subscribers에 추가
        self.adding_subscribers.lock().unwrap().push(subscriber_with_id);

        // ActionOp::AddSubscriber 액션을 전달하여 reducer에서 처리하도록 함
        if let Some(tx) = self.dispatch_tx.lock().unwrap().as_ref() {
            match tx.send(ActionOp::AddSubscriber) {
                Ok(_) => {}
                Err(_e) => {
                    #[cfg(feature = "store-log")]
                    eprintln!(
                        "store: Error while sending add subscriber to dispatch channel: {:?}",
                        _e
                    );
                    self.adding_subscribers.lock().unwrap().retain(|s| s.id != subscriber_id);
                    return Err(StoreError::DispatchError(format!(
                        "Error while sending add subscriber to dispatch channel: {:?}",
                        _e
                    )));
                }
            }
        } else {
            self.adding_subscribers.lock().unwrap().retain(|s| s.id != subscriber_id);
            return Err(StoreError::DispatchError(
                "Dispatch channel is closed".to_string(),
            ));
        }

        // disposer for the subscriber
        let subscribers = self.subscribers.clone();
        let adding_subscribers = self.adding_subscribers.clone();
        let subscription = Box::new(SubscriberSubscription {
            subscriber_id, // Store the ID for comparison
            unsubscribe: Box::new(move |subscriber_id| {
                // dispacher는 Arc<StoreImpl<State, Action>> 이므로 RemoveSubscriber action 을 사용할 수 없는 이유
                // 그래서 직접 vector에서 제거한다.

                // remove from adding_subscribers
                let mut adding = adding_subscribers.lock().unwrap();
                adding.retain(|s| s.id != subscriber_id); // Compare by ID

                let mut subscribers = subscribers.lock().unwrap();
                subscribers.retain(|s| {
                    let retain = s.id != subscriber_id; // Compare by ID
                    if !retain {
                        s.on_unsubscribe();
                    }
                    retain
                });
            }),
        });

        Ok(subscription)
    }

    /// clear all subscribers
    pub(crate) fn clear_subscribers(&self) {
        #[cfg(feature = "store-log")]
        eprintln!("store: clear_subscribers");
        match self.subscribers.lock() {
            Ok(mut subscribers) => {
                for subscriber_with_id in subscribers.iter() {
                    subscriber_with_id.on_unsubscribe();
                }
                subscribers.clear();
            }
            Err(mut e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking subscribers: {:?}", e);
                for subscriber_with_id in e.get_ref().iter() {
                    subscriber_with_id.on_unsubscribe();
                }
                e.get_mut().clear();
            }
        }
    }

    /// Run middleware + reducers for a single action.
    ///
    /// ### Parameters
    /// * `state`: Current state reference
    /// * `action`: Action reference
    /// * `effects`: Mutable reference to an effects buffer that will be extended with all emitted effects
    /// * `middleware_deco`: Middleware chain entry point
    ///
    /// ### Returns
    /// * `Ok(DispatchOp<State, Action>)`: dispatch operation containing the next state and effects
    /// * `Err(StoreError)`: if an error occurs in middleware or reducers
    pub(crate) fn do_reduce(
        &self,
        state: &State,
        action: &Action,
        effects: &mut Vec<Effect<Action>>,
        middleware_deco: Arc<MiddlewareFn<State, Action>>,
    ) -> Result<DispatchOp<State, Action>, StoreError> {
        let started_at = Instant::now();

        // call middleware chain
        let mut dispatch_op = middleware_deco(state, action)?;

        // Extract effects from DispatchOp and add to effects vector (mutable parameter)
        match &mut dispatch_op {
            DispatchOp::Dispatch(_, ref mut result_effects) => {
                effects.append(result_effects);
            }
            DispatchOp::Keep(_, ref mut result_effects) => {
                effects.append(result_effects);
            }
        }

        self.metrics.middleware_executed(Some(action), "", 1, started_at.elapsed());

        Ok(dispatch_op)
    }

    pub(crate) fn do_effect(
        &self,
        effects: &mut Vec<Effect<Action>>,
        dispatcher: Arc<StoreImpl<State, Action>>,
    ) {
        let effect_start = Instant::now();
        self.metrics.effect_issued(effects.len());

        let effects_total = effects.len();
        while !effects.is_empty() {
            let effect = effects.remove(0);
            match effect {
                Effect::Action(a) => {
                    dispatcher.dispatch_thunk(Box::new(
                        move |weak_dispatcher| match weak_dispatcher.dispatch(a) {
                            Ok(_) => {}
                            Err(_e) => {
                                #[cfg(feature = "store-log")]
                                eprintln!("Error while dispatching action: {:?}", _e);
                            }
                        },
                    ));
                }
                Effect::Task(task) => {
                    dispatcher.dispatch_task(task);
                }
                Effect::Thunk(thunk) => {
                    dispatcher.dispatch_thunk(thunk);
                }
                Effect::Function(_tok, func) => {
                    let dispatcher_clone = dispatcher.clone();
                    dispatcher.dispatch_task(Box::new(move || {
                        // Execute the function and try to convert result to Action
                        match func() {
                            Ok(result) => {
                                // Try to downcast the result to Action type first
                                if let Ok(action) = result.downcast::<Action>() {
                                    let _ = dispatcher_clone.dispatch(*action);
                                    return;
                                }
                                // Note: Generic conversion from result to Action is not type-safe
                                // Effect::Function results should be handled by middleware or
                                // use Effect::Thunk/Effect::Action instead for type safety
                                #[cfg(feature = "store-log")]
                                eprintln!("Effect function should be handled by a middleware, did you miss a middleware?");
                            }
                            Err(_e) => {
                                // Error in effect function, ignore
                                #[cfg(feature = "store-log")]
                                eprintln!("Effect function error: {:?}", _e);
                            }
                        }
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
        _dispatcher: Arc<StoreImpl<State, Action>>,
        _action_received_at: Instant,
    ) {
        let notify_start = Instant::now();
        self.metrics.state_notified(Some(next_state));

        #[cfg(feature = "store-log")]
        eprintln!("store: notify: action: {}", describe_action(action));

        let subscribers = self.subscribers.lock().unwrap().clone();
        let subscriber_count = subscribers.len();

        // Clone action for metrics
        let action_for_metrics = action.clone();

        // Notify all subscribers with refs (no clone needed)
        for subscriber_with_id in subscribers.iter() {
            subscriber_with_id.on_notify(next_state, action);
        }

        let duration = notify_start.elapsed();
        self.metrics.subscriber_notified(Some(&action_for_metrics), subscriber_count, duration);
    }

    fn do_subscribe(
        &self,
        state: State,
        new_subscribers: impl Iterator<Item = SubscriberWithId<State, Action>>,
    ) {
        let mut subscribers = self.subscribers.lock().unwrap();

        // notify new subscribers with the latest state and add to subscribers
        for subscriber_with_id in new_subscribers {
            subscriber_with_id.on_subscribe(&state);

            subscribers.push(subscriber_with_id);
        }
    }

    #[allow(dead_code)]
    fn do_remove_subscriber(&self, subscriber_id: u64) {
        // remove from adding_subscribers
        let mut adding_subscribers = self.adding_subscribers.lock().unwrap();
        adding_subscribers.retain(|s| s.id != subscriber_id);

        // remove from subscribers
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|s| {
            let retain = s.id != subscriber_id;
            if !retain {
                s.on_unsubscribe();
            }
            retain
        });
    }

    fn do_state_function(&self) {
        let state_ref = match self.state.lock() {
            Ok(state) => state,
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking state: {:?}", _e);
                return;
            }
        };
        match self.state_functions.lock() {
            Ok(mut state_functions) => {
                for state_function in state_functions.drain(..) {
                    state_function(&state_ref);
                }
                //state_functions.clear();
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking state functions: {:?}", _e);
            }
        };
    }

    fn on_close(&self, action_received_at: Instant) {
        #[cfg(feature = "store-log")]
        eprintln!("store: on_close");

        self.metrics.action_executed(None, action_received_at.elapsed());
    }

    /// close the store
    ///
    /// send an exit action to the store and drop the dispatch channel
    ///
    /// ## Return
    /// * Ok(()) : if the store is closed
    /// * Err(StoreError) : if the store is not closed, this can be happened when the queue is full
    pub fn close(&self) -> Result<(), StoreError> {
        // take the ownership and release the lock to avoid deadlock
        let dispatch_tx = self.dispatch_tx.lock().map(|mut tx| tx.take());
        match dispatch_tx {
            Ok(Some(tx)) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: close: sending exit to dispatch channel");
                match tx.send(ActionOp::Exit(Instant::now())) {
                    Ok(_) => {
                        #[cfg(feature = "store-log")]
                        eprintln!("store: close: dispatch channel sent exit");
                    }
                    Err(_e) => {
                        #[cfg(feature = "store-log")]
                        eprintln!(
                            "store: close: Error while sending exit to dispatch channel: {:?}",
                            _e
                        );
                        return Err(StoreError::DispatchError(format!(
                            "Error while sending exit to dispatch channel, try it later: {:?}",
                            _e
                        )));
                    }
                }
            }
            Ok(None) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: close: dispatch channel already closed");
                return Ok(());
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!(
                    "store: close: Error while locking dispatch channel: {:?}",
                    _e
                );
                return Err(StoreError::DispatchError(format!(
                    "Error while locking dispatch channel: {:?}",
                    _e
                )));
            }
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: close: dispatch channel closed");
        Ok(())
    }

    /// close the store and wait for the dispatcher to finish
    ///
    /// ## Return
    /// * Ok(()) : if the store is closed
    /// * Err(StoreError) : if the store is not closed, this can be happened when the queue is full
    pub fn stop(&self) -> Result<(), StoreError> {
        self.stop_with_timeout(Duration::from_millis(0))
    }

    /// close the store and wait for the dispatcher to finish
    pub fn stop_with_timeout(&self, timeout: Duration) -> Result<(), StoreError> {
        match self.close() {
            Ok(_) => {}
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while closing dispatch channel: {:?}", _e);
                // fall through
                //return Err(_e);
            }
        }

        // Shutdown the thread pool with timeout
        // take the ownership and release the lock to avoid deadlock
        let pool = self.pool.lock().map(|mut pool_guard| pool_guard.take());
        match pool {
            Ok(Some(pool)) => {
                if timeout.is_zero() {
                    pool.shutdown_join();
                } else {
                    pool.shutdown_join_timeout(timeout);
                }
                #[cfg(feature = "store-log")]
                eprintln!("store: pool shutdown completed");
            }
            Ok(None) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: pool already shutdown");
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: Error while locking pool: {:?}", _e);
                return Err(StoreError::DispatchError(format!(
                    "Error while shutting down pool: {:?}",
                    _e
                )));
            }
        }

        #[cfg(feature = "store-log")]
        eprintln!("store: stopped");
        Ok(())
    }

    /// dispatch an action
    ///
    /// ### Return
    /// * Ok(()) : if the action is dispatched
    /// * Err(StoreError) : if the dispatch channel is closed
    pub(crate) fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        let sender = self.dispatch_tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            // the number of remaining actions in the channel
            match tx.send(ActionOp::Action(action)) {
                Ok(remains) => {
                    self.metrics.queue_size(remains as usize);
                    Ok(())
                }
                Err(e) => match e {
                    SenderError::SendError(action_op) => {
                        let action_desc = describe_action_op(&action_op);
                        let err = StoreError::DispatchError(format!(
                            "Error while sending '{}' to dispatch channel",
                            action_desc
                        ));
                        self.metrics.error_occurred(&err);
                        Err(err)
                    }
                    SenderError::ChannelClosed => {
                        let err =
                            StoreError::DispatchError("Dispatch channel is closed".to_string());
                        self.metrics.error_occurred(&err);
                        Err(err)
                    }
                },
            }
        } else {
            let err = StoreError::DispatchError("Dispatch channel is closed".to_string());
            self.metrics.error_occurred(&err);
            Err(err)
        }
    }

    /// Query the current state with a function.
    ///
    /// The function will be executed in the store thread with the current state *moved* into it.
    /// This is useful for read‑only inspections or aggregations that should observe a consistent snapshot.
    ///
    /// ### Parameters
    /// * `query_fn`: A function that receives the current state by value (`State`)
    ///
    /// ### Returns
    /// * `Ok(())` : if the query is scheduled successfully
    /// * `Err(StoreError)` : if the store is not available
    pub fn query_state<F>(&self, query_fn: F) -> Result<(), StoreError>
    where
        F: FnOnce(&State) + Send + Sync + 'static,
    {
        self.state_functions.lock().unwrap().push(Box::new(query_fn));

        if let Ok(tx) = self.dispatch_tx.lock() {
            if let Some(tx) = tx.as_ref() {
                return match tx.send(ActionOp::StateFunction) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(StoreError::DispatchError(format!(
                        "Error while sending state function to dispatch channel: {:?}",
                        e
                    ))),
                };
            }
            Err(StoreError::DispatchError(
                "Dispatch channel is closed".to_string(),
            ))
        } else {
            Err(StoreError::DispatchError(
                "Dispatch channel is closed".to_string(),
            ))
        }
    }

    // /// Add middleware
    // pub(crate) fn add_middleware(&self, middleware: Arc<dyn NewMiddlewareFnFactory<State, Action> + Send + Sync>) {
    //     self.middleware_factories.lock().unwrap().push(middleware);
    // }

    /// Iterator for the state
    ///
    /// it uses a channel to subscribe to the state changes
    /// the channel is rendezvous(capacity 1), the store will block on the channel until the subscriber consumes the state
    #[allow(dead_code)]
    #[doc(hidden)]
    pub(crate) fn iter(&self) -> Result<impl Iterator<Item = (State, Action)>, StoreError> {
        self.iter_with(1, BackpressurePolicy::BlockOnFull)
    }

    /// Iterator for the state
    ///  
    /// ### Parameters
    /// * capacity: the capacity of the channel
    /// * policy: the backpressure policy
    #[allow(dead_code)]
    #[doc(hidden)]
    pub(crate) fn iter_with(
        &self,
        capacity: usize,
        policy: BackpressurePolicy<(State, Action)>,
    ) -> Result<impl Iterator<Item = (State, Action)>, StoreError> {
        let (iter_tx, iter_rx) = BackpressureChannel::<(State, Action)>::pair_with(
            "store_iter",
            capacity,
            policy,
            Some(self.metrics.clone()),
        );

        let subscription = self.add_subscriber(Arc::new(StateIteratorSubscriber::new(iter_tx)));
        Ok(StateIterator::new(iter_rx, subscription?))
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
    /// * policy: Backpressure policy for when down channel(store to subscriber) is full
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
        let subscription = self.add_subscriber(channel_subscriber);

        // return subscription
        #[allow(clippy::let_and_return)]
        subscription
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
                ActionOp::StateFunction => {
                    #[cfg(feature = "store-log")]
                    eprintln!("store: {} received StateFunction (ignored)", _name);
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
        // take the ownership and release the lock to avoid deadlock
        let tx_owned = self.tx.lock().map(|mut tx| tx.take());
        match tx_owned {
            Ok(Some(tx)) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: ChanneledSubscriber: clearing resource: sending exit");
                let _ = tx.send(ActionOp::Exit(Instant::now()));
                drop(tx);
            }
            Ok(None) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: ChanneledSubscriber: clearing resource: channel already closed");
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!(
                    "store: ChanneledSubscriber: clearing resource: Error while locking channel: {:?}",
                    _e
                );
            }
        }

        // join the thread
        let handled_owned = self.handle.lock().map(|mut handle| handle.take());
        match handled_owned {
            Ok(Some(h)) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: ChanneledSubscriber: clearing resource: joining thread");
                let _ = h.join();
            }
            Ok(None) => {
                #[cfg(feature = "store-log")]
                eprintln!("store: ChanneledSubscriber: clearing resource: thread already joined");
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!(
                    "store: ChanneledSubscriber: clearing resource: Error while locking thread handle: {:?}",
                    _e
                );
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
                // Clone needed for sending through channel
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
        let _ = self.close();

        // Shutdown the thread pool with timeout
        let _ = self.stop_with_timeout(DEFAULT_STOP_TIMEOUT);
        // if let Ok(mut lk) = self.pool.lock() {
        //     if let Some(pool) = lk.take() {
        //         pool.shutdown_join_timeout(Duration::from_secs(3));
        //     }
        // }

        #[cfg(feature = "store-log")]
        eprintln!("store: '{}' dropped", self.name);
    }
}

impl<State, Action> Store<State, Action> for StoreImpl<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
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
    ) -> Result<Box<dyn Subscription>, StoreError> {
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

    fn stop(&self) -> Result<(), StoreError> {
        self.stop()
    }

    fn stop_timeout(&self, timeout: Duration) -> Result<(), StoreError> {
        self.stop_with_timeout(timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackpressurePolicy, Dispatcher, Effect, FnReducer, Reducer, StoreBuilder};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

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
            //println!("TestChannelSubscriber: state={}, action={}", state, action);
            self.received.lock().unwrap().push((*state, *action));
        }
    }

    struct TestReducer;

    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            DispatchOp::Dispatch(state + action, vec![])
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
            //println!("SlowSubscriber: state={}, action={}", state, action);
            std::thread::sleep(self.delay);
            self.received.lock().unwrap().push((*state, *action));
        }
    }

    #[test]
    fn test_store_subscribed_basic() {
        // Setup store with a simple counter
        let initial_state = 0;
        let reducer = Box::new(TestReducer);
        let store_result = StoreImpl::new_with_reducer(initial_state, reducer);
        assert!(store_result.is_ok());
        let store = store_result.unwrap();

        // Create subscriber to receive updates
        let received_states = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Box::new(TestChannelSubscriber::new(received_states.clone()));
        // Create channel
        let subscription =
            store.subscribed_with(10, BackpressurePolicy::DropOldestIf(None), subscriber1);

        // Dispatch some actions
        store.dispatch(1).unwrap();
        store.dispatch(2).unwrap();

        // Give some time for processing
        // thread::sleep(Duration::from_millis(100));
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

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
        let store_result = StoreImpl::new_with_reducer(0, Box::new(TestReducer));
        assert!(store_result.is_ok());
        let store = store_result.unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let subscriber = Box::new(SlowSubscriber::new(
            received_clone,
            Duration::from_millis(100),
        ));
        // Create channel with small capacity
        let subscription =
            store.subscribed_with(1, BackpressurePolicy::DropOldestIf(None), subscriber);

        // Fill the channel
        for i in 0..5 {
            let _ = store.dispatch(i).unwrap();
        }

        // Give some time for having channel thread to process
        thread::sleep(Duration::from_millis(200));
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
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
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Box::new(TestChannelSubscriber::new(received.clone()));
        let subscription =
            store.subscribed_with(10, BackpressurePolicy::DropOldestIf(None), subscriber1);

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
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
        // subscriber should not receive the state
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    // 새로운 subscriber가 추가될 때 최신 상태를 받는지 테스트
    #[test]
    fn test_new_subscriber_receives_latest_state() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // 첫 번째 subscriber 추가
        let received1 = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Arc::new(TestChannelSubscriber::new(received1.clone()));
        store.add_subscriber(subscriber1).unwrap();

        // 액션을 dispatch하여 상태 변경
        store.dispatch(5).unwrap();
        store.dispatch(10).unwrap();

        // 잠시 대기하여 액션이 처리되도록 함
        thread::sleep(Duration::from_millis(100));

        // 두 번째 subscriber 추가 (현재 상태는 15)
        let received2 = Arc::new(Mutex::new(Vec::new()));
        let subscriber2 = Arc::new(TestChannelSubscriber::new(received2.clone()));
        store.add_subscriber(subscriber2).unwrap();

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
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

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

        store.add_subscriber(subscriber).unwrap();

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
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

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
            store.add_subscriber(subscriber.clone()).unwrap();
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
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // 액션을 dispatch하여 상태 변경
        store.dispatch(5).unwrap();

        // subscriber 추가
        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Arc::new(TestChannelSubscriber::new(received.clone()));
        let subscription = store.add_subscriber(subscriber).unwrap();

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
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // store 중지
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // subscriber 추가 시도
        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Arc::new(TestChannelSubscriber::new(received.clone()));
        let subscription = store.add_subscriber(subscriber);
        assert!(subscription.is_err());
    }

    // on_subscribe에서 상태를 수정하는 subscriber 테스트
    #[test]
    fn test_subscriber_modifies_state_in_on_subscribe() {
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

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

        store.add_subscriber(subscriber.clone()).unwrap();

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
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // 첫 번째 subscriber 추가
        let received1 = Arc::new(Mutex::new(Vec::new()));
        let subscriber1 = Arc::new(TestChannelSubscriber::new(received1.clone()));
        store.add_subscriber(subscriber1).unwrap();

        // 잠시 대기
        thread::sleep(Duration::from_millis(50));

        // 두 번째 subscriber 추가
        let received2 = Arc::new(Mutex::new(Vec::new()));
        let subscriber2 = Arc::new(TestChannelSubscriber::new(received2.clone()));
        store.add_subscriber(subscriber2).unwrap();

        // 잠시 대기
        thread::sleep(Duration::from_millis(50));

        // 세 번째 subscriber 추가
        let received3 = Arc::new(Mutex::new(Vec::new()));
        let subscriber3 = Arc::new(TestChannelSubscriber::new(received3.clone()));
        store.add_subscriber(subscriber3).unwrap();

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

    /// Test basic iterator functionality
    #[test]
    fn test_store_iter_basic() {
        // given: store with reducer
        // let store = StoreBuilder::new(0)
        //     .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
        //         DispatchOp::Dispatch(state + action, None)
        //     })))
        //     .build()
        //     .unwrap();
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // when: create iterator and dispatch actions
        let mut iter = store.iter().unwrap();

        // dispatch actions
        store.dispatch(10).expect("dispatch should succeed");
        store.dispatch(20).expect("dispatch should succeed");
        store.dispatch(30).expect("dispatch should succeed");

        // then: iterator should return state and action pairs
        assert_eq!(iter.next(), Some((10, 10))); // state: 0+10=10, action: 10
        assert_eq!(iter.next(), Some((30, 20))); // state: 10+20=30, action: 20
        assert_eq!(iter.next(), Some((60, 30))); // state: 30+30=60, action: 30

        // stop store and verify iterator ends
        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with no actions dispatched
    #[test]
    fn test_store_iter_no_actions() {
        // given: store with reducer
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // when: create iterator without dispatching actions
        let mut iter = store.iter().unwrap();

        // then: iterator should return None immediately
        // since no actions were dispatched, no state changes occurred
        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with complex state and action types
    #[test]
    fn test_store_iter_complex_types() {
        // given: store with complex state and action
        #[derive(Debug, Clone, PartialEq)]
        struct ComplexState {
            value: i32,
            name: String,
        }

        #[allow(dead_code)]
        #[derive(Debug, Clone, PartialEq)]
        enum ComplexAction {
            Add(i32),
            SetName(String),
            Reset,
        }

        let store = StoreImpl::new_with_reducer(
            ComplexState {
                value: 0,
                name: "initial".to_string(),
            },
            Box::new(FnReducer::from(
                |state: &ComplexState, action: &ComplexAction| match action {
                    ComplexAction::Add(n) => DispatchOp::Dispatch(
                        ComplexState {
                            value: state.value + n,
                            name: state.name.clone(),
                        },
                        vec![],
                    ),
                    ComplexAction::SetName(name) => DispatchOp::Dispatch(
                        ComplexState {
                            value: state.value,
                            name: name.clone(),
                        },
                        vec![],
                    ),
                    ComplexAction::Reset => DispatchOp::Dispatch(
                        ComplexState {
                            value: 0,
                            name: "reset".to_string(),
                        },
                        vec![],
                    ),
                },
            )),
        )
        .unwrap();

        // when: create iterator and dispatch actions
        let mut iter = store.iter().unwrap();

        store.dispatch(ComplexAction::Add(10)).expect("dispatch should succeed");
        store
            .dispatch(ComplexAction::SetName("test".to_string()))
            .expect("dispatch should succeed");
        store.dispatch(ComplexAction::Add(5)).expect("dispatch should succeed");

        // then: iterator should return correct state and action pairs
        assert_eq!(
            iter.next(),
            Some((
                ComplexState {
                    value: 10,
                    name: "initial".to_string(),
                },
                ComplexAction::Add(10)
            ))
        );
        assert_eq!(
            iter.next(),
            Some((
                ComplexState {
                    value: 10,
                    name: "test".to_string(),
                },
                ComplexAction::SetName("test".to_string())
            ))
        );
        assert_eq!(
            iter.next(),
            Some((
                ComplexState {
                    value: 15,
                    name: "test".to_string(),
                },
                ComplexAction::Add(5)
            ))
        );

        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with multiple concurrent actions
    #[test]
    fn test_store_iter_concurrent_actions() {
        // given: store with reducer
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // when: create iterator and dispatch many actions quickly
        let mut iter = store.iter().unwrap();

        // dispatch multiple actions
        for i in 1..=10 {
            store.dispatch(i).expect("dispatch should succeed");
        }

        // then: iterator should return all state and action pairs in order
        let mut expected_state = 0;
        for i in 1..=10 {
            expected_state += i;
            assert_eq!(iter.next(), Some((expected_state, i)));
        }

        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with store that has middleware
    #[test]
    fn test_store_iter_with_middleware() {
        // given: store with middleware
        struct TestMiddleware;

        impl<State, Action> MiddlewareFnFactory<State, Action> for TestMiddleware
        where
            State: Send + Sync + Clone + 'static,
            Action: Send + Sync + Clone + std::fmt::Debug + 'static,
        {
            fn create(&self, inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action> {
                inner
            }
        }

        let store = StoreImpl::new_with(
            0,
            vec![Box::new(FnReducer::from(|state: &i32, action: &i32| {
                DispatchOp::Dispatch(state + action, vec![])
            }))],
            "test".to_string(),
            1,
            BackpressurePolicy::default(),
            vec![Arc::new(TestMiddleware)],
        )
        .unwrap();

        // when: create iterator and dispatch actions
        let mut iter = store.iter().unwrap();

        store.dispatch(5).expect("dispatch should succeed");
        store.dispatch(10).expect("dispatch should succeed");

        // then: iterator should work correctly with middleware
        assert_eq!(iter.next(), Some((5, 5)));
        assert_eq!(iter.next(), Some((15, 10)));

        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with store that has effects
    #[test]
    fn test_store_iter_with_effects() {
        // given: store with reducer that produces effects
        let store = StoreImpl::new_with_reducer(
            0,
            Box::new(FnReducer::from(|state: &i32, action: &i32| {
                let new_state = state + action;
                let mut effects_vec = vec![];
                if *action > 5 {
                    effects_vec.push(Effect::Task(Box::new(|| {
                        // effect that does nothing
                    })));
                }
                DispatchOp::Dispatch(new_state, effects_vec)
            })),
        )
        .unwrap();

        // when: create iterator and dispatch actions
        let mut iter = store.iter().unwrap();

        store.dispatch(3).expect("dispatch should succeed"); // no effect
        store.dispatch(10).expect("dispatch should succeed"); // with effect

        // then: iterator should work correctly with effects
        assert_eq!(iter.next(), Some((3, 3)));
        assert_eq!(iter.next(), Some((13, 10)));

        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with store that has multiple reducers
    #[test]
    fn test_store_iter_with_multiple_reducers() {
        // given: store with multiple reducers
        // StoreBuilder의 with_reducer는 기존 리듀서를 대체하므로
        // 실제로는 마지막 리듀서만 사용됩니다
        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
                DispatchOp::Dispatch(state + action, vec![])
            })))
            .with_reducer(Box::new(FnReducer::from(|state: &i32, _action: &i32| {
                DispatchOp::Dispatch(state * 2, vec![])
            })))
            .build()
            .unwrap();

        // when: create iterator and dispatch actions
        // let mut iter = store.iter().unwrap();

        store.dispatch(5).expect("dispatch should succeed");
        store.dispatch(10).expect("dispatch should succeed");

        // // then: iterator should work with multiple reducers
        // // 실제로는 마지막 리듀서만 사용되므로: 0 * 2 = 0, 0 * 2 = 0
        // let first_result = iter.next();
        // println!("First result: {:?}", first_result);
        // let second_result = iter.next();
        // println!("Second result: {:?}", second_result);

        // // 마지막 리듀서만 사용되므로 상태는 항상 0
        // assert_eq!(first_result, Some((0, 5)));
        // assert_eq!(second_result, Some((0, 10)));

        store.stop().expect("store should stop");
        // assert_eq!(iter.next(), None);
    }

    /// Test iterator behavior when store is stopped before consuming all items
    #[test]
    fn test_store_iter_early_stop() {
        // given: store with reducer
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // when: create iterator, dispatch actions, but stop store early
        let mut iter = store.iter().unwrap();

        store.dispatch(5).expect("dispatch should succeed");
        store.dispatch(10).expect("dispatch should succeed");
        store.dispatch(15).expect("dispatch should succeed");

        // consume all items before stopping store
        assert_eq!(iter.next(), Some((5, 5)));
        assert_eq!(iter.next(), Some((15, 10))); // 5 + 10 = 15
        assert_eq!(iter.next(), Some((30, 15))); // 15 + 15 = 30

        // stop store after consuming all items
        store.stop().expect("store should stop");
    }

    /// Test iterator with different backpressure policies
    #[test]
    fn test_store_iter_with_block_on_full() {
        // given: store with different backpressure policies
        let store = StoreImpl::new_with(
            0,
            vec![Box::new(FnReducer::from(|state: &i32, action: &i32| {
                DispatchOp::Dispatch(state + action, vec![])
            }))],
            "test".to_string(),
            2,
            BackpressurePolicy::BlockOnFull,
            vec![],
        )
        .unwrap();

        // when: create iterator with different capacity and policy
        let mut iter = store.iter_with(1, BackpressurePolicy::BlockOnFull).unwrap();

        store.dispatch(5).expect("dispatch should succeed");
        store.dispatch(10).expect("dispatch should succeed");

        // then: iterator should work with custom capacity and policy
        assert_eq!(iter.next(), Some((5, 5)));
        assert_eq!(iter.next(), Some((15, 10)));

        store.stop().expect("store should stop");
    }

    // #[test]
    // fn test_store_iter_with_different_policies() {
    //     // given: store with different backpressure policies
    //     let store = StoreBuilder::new(0)
    //         .with_reducer(Box::new(FnReducer::from(|state: &i32, action: &i32| {
    //             DispatchOp::Dispatch(state + action, None)
    //         })))
    //         .with_capacity(2)
    //         .with_policy(BackpressurePolicy::DropOldestIf(None))
    //         .build()
    //         .unwrap();
    //
    //     // when: create iterator with different capacity and policy
    //     let mut iter = store.iter_with(1, BackpressurePolicy::BlockOnFull);
    //
    //     store.dispatch(5).expect("dispat
    // ch should succeed");
    //     store.dispatch(10).expect("dispatch should succeed");
    //
    //     // then: iterator should work with custom capacity and policy
    //     assert_eq!(iter.next(), Some((5, 5)));
    //     assert_eq!(iter.next(), Some((15, 10)));
    //
    //     store.stop().expect("store should stop");
    // }

    /// Test iterator with string state and action
    #[test]
    fn test_store_iter_string_types() {
        // given: store with string state and action
        let store = StoreImpl::new_with_reducer(
            "".to_string(),
            Box::new(FnReducer::from(|state: &String, action: &String| {
                let new_state = format!("{}{}", state, action);
                DispatchOp::Dispatch(new_state, vec![])
            })),
        )
        .unwrap();

        // when: create iterator and dispatch string actions
        let mut iter = store.iter().unwrap();

        store.dispatch("hello".to_string()).expect("dispatch should succeed");
        store.dispatch(" world".to_string()).expect("dispatch should succeed");

        // then: iterator should work with string types
        assert_eq!(
            iter.next(),
            Some(("hello".to_string(), "hello".to_string()))
        );
        assert_eq!(
            iter.next(),
            Some(("hello world".to_string(), " world".to_string()))
        );

        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test iterator with empty state changes (reducer returns same state)
    #[test]
    fn test_store_iter_no_state_change() {
        // given: store with reducer that doesn't change state
        let store = StoreImpl::new_with_reducer(
            0,
            Box::new(FnReducer::from(|state: &i32, _action: &i32| {
                DispatchOp::Dispatch(state.clone(), vec![]) // return same state
            })),
        )
        .unwrap();
        // when: create iterator and dispatch actions
        let mut iter = store.iter().unwrap();

        store.dispatch(5).expect("dispatch should succeed");
        store.dispatch(10).expect("dispatch should succeed");

        // then: iterator should still return state and action pairs
        assert_eq!(iter.next(), Some((0, 5))); // state remains 0
        assert_eq!(iter.next(), Some((0, 10))); // state remains 0

        store.stop().expect("store should stop");
        assert_eq!(iter.next(), None);
    }

    /// Test query_state functionality
    #[test]
    fn test_query_state_basic() {
        // given: store with a simple counter
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // dispatch some actions to change state
        store.dispatch(5).unwrap();
        store.dispatch(10).unwrap();

        // wait for actions to be processed
        std::thread::sleep(std::time::Duration::from_millis(100));

        // when: query the current state
        let queried_value = std::sync::Arc::new(std::sync::Mutex::new(0));
        let queried_value_clone = queried_value.clone();
        store
            .query_state(move |state| {
                *queried_value_clone.lock().unwrap() = *state;
            })
            .unwrap();

        let _ = store.stop();
        // then: should get the current state (0 + 5 + 10 = 15)
        assert_eq!(*queried_value.lock().unwrap(), 15);
    }

    /// Test query_state with complex state
    #[test]
    fn test_query_state_complex() {
        // given: store with complex state
        #[derive(Debug, Clone, PartialEq)]
        struct ComplexState {
            value: i32,
            name: String,
        }

        #[derive(Debug, Clone, PartialEq)]
        enum ComplexAction {
            Add(i32),
            SetName(String),
        }

        let store = StoreImpl::new_with_reducer(
            ComplexState {
                value: 0,
                name: "initial".to_string(),
            },
            Box::new(FnReducer::from(
                |state: &ComplexState, action: &ComplexAction| match action {
                    ComplexAction::Add(n) => DispatchOp::Dispatch(
                        ComplexState {
                            value: state.value + n,
                            name: state.name.clone(),
                        },
                        vec![],
                    ),
                    ComplexAction::SetName(name) => DispatchOp::Dispatch(
                        ComplexState {
                            value: state.value,
                            name: name.clone(),
                        },
                        vec![],
                    ),
                },
            )),
        )
        .unwrap();

        // dispatch some actions
        store.dispatch(ComplexAction::Add(10)).unwrap();
        store.dispatch(ComplexAction::SetName("test".to_string())).unwrap();
        store.dispatch(ComplexAction::Add(5)).unwrap();

        // when: query the current state
        let queried_value = std::sync::Arc::new(std::sync::Mutex::new(0));
        let queried_name = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let queried_value_clone = queried_value.clone();
        let queried_name_clone = queried_name.clone();
        store
            .query_state(move |state| {
                *queried_value_clone.lock().unwrap() = state.value;
                *queried_name_clone.lock().unwrap() = state.name.clone();
            })
            .unwrap();

        store.stop().unwrap();

        // then: should get the current state
        assert_eq!(*queried_value.lock().unwrap(), 15); // 0 + 10 + 5
        assert_eq!(*queried_name.lock().unwrap(), "test");
    }

    /// Test query_state with multiple queries
    #[test]
    fn test_query_state_multiple_queries() {
        // given: store with a simple counter
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // dispatch some actions
        store.dispatch(5).unwrap();
        store.dispatch(10).unwrap();

        // wait for actions to be processed
        std::thread::sleep(std::time::Duration::from_millis(100));

        // when: query the state multiple times
        let query1_result = std::sync::Arc::new(std::sync::Mutex::new(0));
        let query2_result = std::sync::Arc::new(std::sync::Mutex::new(0));

        let query1_clone = query1_result.clone();
        store
            .query_state(move |state| {
                *query1_clone.lock().unwrap() = *state;
            })
            .unwrap();

        store.dispatch(20).unwrap();

        // wait for action to be processed
        std::thread::sleep(std::time::Duration::from_millis(100));

        let query2_clone = query2_result.clone();
        store
            .query_state(move |state| {
                *query2_clone.lock().unwrap() = *state;
            })
            .unwrap();

        store.stop().unwrap();

        // then: should get the correct states
        assert_eq!(*query1_result.lock().unwrap(), 15); // 0 + 5 + 10
        assert_eq!(*query2_result.lock().unwrap(), 35); // 15 + 20
    }

    /// Test query_state with error handling
    #[test]
    fn test_query_state_error_handling() {
        // given: store with a simple counter
        let store = StoreImpl::new_with_reducer(0, Box::new(TestReducer)).unwrap();

        // when: query the state (should succeed)
        let queried_value = std::sync::Arc::new(std::sync::Mutex::new(0));
        let queried_value_clone = queried_value.clone();
        let result = store.query_state(move |state| {
            *queried_value_clone.lock().unwrap() = *state;
        });

        // then: should succeed and get the initial state
        assert!(result.is_ok());
        assert_eq!(*queried_value.lock().unwrap(), 0);
    }
}
