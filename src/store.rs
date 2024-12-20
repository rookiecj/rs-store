use crate::channel::{BackpressureChannel, BackpressurePolicy, SenderChannel};
use crate::dispatcher::Dispatcher;
use crate::middleware::Middleware;
use crate::{DispatchOp, Effect, MiddlewareOp, Reducer, Subscriber, Subscription};
use fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fmt, panic, thread};

/// Default capacity for the channel
pub const DEFAULT_CAPACITY: usize = 16;

/// StoreError represents an error that occurred in the store
#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreError {
    #[error("dispatch error: {0}")]
    DispatchError(String),
    #[error("reducer error: {0}")]
    ReducerError(String),
    #[error("subscription error: {0}")]
    SubscriptionError(String),
}

pub(crate) enum ActionOp<Action>
where
    Action: Send + Sync + 'static,
{
    Action(Action),
    Exit,
}

/// Store is a simple implementation of a Redux store
pub struct Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    name: String,
    state: Mutex<State>,
    pub(crate) reducers: Mutex<Vec<Box<dyn Reducer<State, Action> + Send + Sync>>>,
    pub(crate) subscribers: Arc<Mutex<Vec<Arc<dyn Subscriber<State, Action> + Send + Sync>>>>,
    pub(crate) tx: Mutex<Option<SenderChannel<ActionOp<Action>>>>,
    // reduce and dispatch thread
    dispatch_thread: Mutex<Option<thread::JoinHandle<()>>>,
    middlewares: Mutex<Vec<Arc<Mutex<dyn Middleware<State, Action> + Send + Sync>>>>,
}

impl<State, Action> Default for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    fn default() -> Store<State, Action> {
        Store {
            name: "store".to_string(),
            state: Default::default(),
            reducers: Mutex::new(Vec::default()),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            tx: Mutex::new(None),
            dispatch_thread: Mutex::new(None),
            middlewares: Mutex::new(Vec::default()),
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
    Action: Send + Sync + 'static,
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
        )
    }

    /// create a new store with name
    pub fn new_with(
        reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
        state: State,
        name: String,
        capacity: usize,
        policy: BackpressurePolicy,
        middleware: Vec<Arc<Mutex<dyn Middleware<State, Action> + Send + Sync>>>,
    ) -> Result<Arc<Store<State, Action>>, StoreError> {
        let (tx, rx) = BackpressureChannel::<ActionOp<Action>>::new(capacity, policy);
        let store = Store {
            name: name.clone(),
            state: Mutex::new(state),
            reducers: Mutex::new(reducers),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            middlewares: Mutex::new(middleware),
            tx: Mutex::new(Some(tx)),
            dispatch_thread: Mutex::new(None),
        };

        // start a thread in which the store will listen for actions
        let rx_store = Arc::new(store);
        let tx_store = rx_store.clone();
        let builder = thread::Builder::new().name(name);
        let r = builder.spawn(move || {
            while let Some(action) = rx.recv() {
                match action {
                    ActionOp::Action(action) => {
                        // dispacher is implemented as a trait, so it is impossible to pass the store itself as a dispatcher
                        // so we need to pass a cloned store as a dispatcher
                        let the_dispatcher = Arc::new(rx_store.clone());

                        // do reduce
                        let (need_dispatch, effects) =
                            rx_store.do_reduce(&action, the_dispatcher.clone());

                        // do effect remains
                        if let Some(mut effects) = effects {
                            while effects.len() > 0 {
                                let effect = effects.remove(0);
                                match effect {
                                    Effect::Action(a) => {
                                        the_dispatcher.dispatch_thunk(Box::new(
                                            move |dispatcher| {
                                                dispatcher.dispatch(a);
                                            },
                                        ));
                                    }
                                    Effect::Function(f) => {
                                        let _ = the_dispatcher.dispatch_task(f);
                                    }
                                    Effect::Thunk(t) => {
                                        let _ = the_dispatcher.dispatch_thunk(t);
                                    }
                                };
                            }
                        }

                        // do notify subscribers
                        if need_dispatch {
                            rx_store.do_notify(&action);
                        }
                    }
                    ActionOp::Exit => {
                        break;
                    }
                }
            }
        });

        let handle = r.map_err(|e| StoreError::DispatchError(e.to_string()))?;
        tx_store.dispatch_thread.lock().unwrap().replace(handle);
        Ok(tx_store)
    }

    /// get the lastest state(for debugging)
    ///
    /// prefer to use `subscribe` to get the state
    pub fn get_state(&self) -> State {
        self.state.lock().unwrap().clone()
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
        dispatcher: Arc<dyn Dispatcher<Action>>,
    ) -> (bool, Option<Vec<Effect<Action>>>) {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let current_state = self.state.lock().unwrap().clone();

            // Execute before_dispatch for all middlewares
            let mut reduce_action = true;
            let mut skip_index = self.middlewares.lock().unwrap().len();
            for (index, middleware) in self.middlewares.lock().unwrap().iter().enumerate() {
                match middleware.lock().unwrap().before_reduce(
                    action,
                    &current_state,
                    dispatcher.clone(),
                ) {
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
                        skip_index = index;
                        break;
                    }
                    Err(e) => {
                        eprintln!("Middleware error: {:?}", e);
                        //return (false, None);
                    }
                }
            }

            let mut effects = vec![];
            let mut next_state = current_state.clone();
            let mut need_dispatch = true;
            if reduce_action {
                for reducer in self.reducers.lock().unwrap().iter() {
                    match reducer.reduce(&next_state, action) {
                        DispatchOp::Dispatch(new_state, effect) => {
                            next_state = new_state;
                            if let Some(effect) = effect {
                                effects.push(effect);
                            }
                            need_dispatch = true;
                        }
                        DispatchOp::Keep(new_state, effect) => {
                            // keep the state but do not dispatch
                            next_state = new_state;
                            if let Some(effect) = effect {
                                effects.push(effect);
                            }
                            need_dispatch = false;
                        }
                    }
                }
            }

            // Execute after_dispatch for all middlewares in reverse order
            let skip_middlewares = if skip_index != self.middlewares.lock().unwrap().len() {
                self.middlewares.lock().unwrap().len() - skip_index - 1
            } else {
                0
            };
            for middleware in self.middlewares.lock().unwrap().iter().rev().skip(skip_middlewares) {
                match middleware.lock().unwrap().after_reduce(
                    action,
                    &current_state,
                    &next_state,
                    &mut effects,
                    dispatcher.clone(),
                ) {
                    Ok(MiddlewareOp::ContinueAction) => {
                        // do nothing
                    }
                    Ok(MiddlewareOp::DoneAction) => {
                        // do nothing
                    }
                    Ok(MiddlewareOp::BreakChain) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("Middleware error: {:?}", e);
                    }
                }
            }

            *self.state.lock().unwrap() = next_state;
            (need_dispatch, Some(effects))
        }));

        result.unwrap_or_else(|err| {
            // get thread name
            eprintln!("{}: error while reducing {:?}", self.name.clone(), err);
            (false, None)
        })
    }

    pub(crate) fn do_notify(&self, action: &Action) {
        // TODO thread pool
        let subscribers = self.subscribers.lock().unwrap().clone();
        let next_state = self.state.lock().unwrap().clone();
        for subscriber in subscribers.iter() {
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                subscriber.on_notify(&next_state, action)
            }));
            result.unwrap_or_else(|err| {
                eprintln!("{}: error while notifying {:?}", self.name.clone(), err);
            });
        }
    }

    /// close the store
    pub fn close(&self) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            tx.send(ActionOp::Exit).unwrap_or(());
            drop(tx);
        }
    }

    /// close the store and wait for the dispatcher to finish
    pub fn stop(&self) {
        self.close();

        // let handle = self.detach();
        if let Some(handle) = self.detach() {
            handle.join().unwrap_or(());
        }
    }

    /// detach the dispatcher thread from the store
    pub fn detach(&self) -> Option<thread::JoinHandle<()>> {
        self.dispatch_thread.lock().unwrap().take()
    }

    pub fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        let sender = self.tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            tx.send(ActionOp::Action(action)).unwrap_or(());
            Ok(())
        } else {
            Err(StoreError::DispatchError(
                "Dispatch channel is closed".to_string(),
            ))
        }
    }

    /// Add middleware method
    pub fn add_middleware(&self, middleware: Arc<Mutex<dyn Middleware<State, Action>>>) {
        self.middlewares.lock().unwrap().push(middleware);
    }
}

/// close tx channel when the store is dropped, but not the dispatcher
/// if you want to stop the dispatcher, call the stop method
impl<State, Action> Drop for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    fn drop(&mut self) {
        //self.stop();
        self.close();
    }
}

impl<State, Action> Dispatcher<Action> for Arc<Store<State, Action>>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    fn dispatch(&self, action: Action) {
        let sender = self.tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            tx.send(ActionOp::Action(action)).unwrap_or(());
        }
    }

    fn dispatch_thunk(
        &self,
        thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>,
    ) -> thread::JoinHandle<()> {
        let self_clone = self.clone();
        thread::spawn(move || {
            let dispatcher: Box<Arc<Store<State, Action>>> = Box::new(self_clone);
            thunk(dispatcher);
        })
    }

    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            task();
        })
    }
}

pub trait StoreMetrics {
    fn action_processed(&self, action_type: &str, duration: Duration);
    fn state_changed(&self, old_size: usize, new_size: usize);
    fn subscriber_notified(&self, count: usize, duration: Duration);
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_panic_on_reducer() {
        // given
        let reducer = Box::new(PanicReducer);
        let store = Store::new_with_state(reducer, 42);

        // when
        store.dispatch(1);
        //thread::sleep(Duration::from_millis(100));
        store.stop();

        // then
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

        // at this point, the state probably not be changed
        //assert_eq!(store.get_state(), 42);

        // give time to the effect
        thread::sleep(Duration::from_secs(1));
        store.stop();

        assert_eq!(store.get_state(), 43);
    }
}
