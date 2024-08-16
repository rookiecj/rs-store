use std::sync::{Arc, Mutex};
use std::thread;

use crate::channel::{BackpressureChannel, SenderChannel};
use crate::dispatcher::Dispatcher;
use crate::{channel, Reducer, Subscriber, Subscription};

pub const DEFAULT_CAPACITY: usize = 16;
pub const DEFAULT_POLICY: channel::BackpressurePolicy = channel::BackpressurePolicy::BlockOnFull;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreError {
    #[error("no error")]
    NoError,
    #[error("store error: {0}")]
    Error(String),
}

/// determine if the action should be dispatched or not
#[derive(Debug, Clone)]
pub enum DispatchOp<State> {
    /// Dispatch new state
    Dispatch(State),
    /// Keep new state but do not dispatch
    Keep(State),
}

// #[derive(Clone,Debug)]
pub enum ActionOp<A>
where
    A: Send + Sync + 'static,
{
    Action(A),
    Exit,
}

/// Store is a simple implementation of a Redux store
pub struct Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    state: Mutex<State>,
    pub(crate) reducers: Mutex<Vec<Box<dyn Reducer<State, Action> + Send + Sync>>>,
    pub(crate) subscribers: Arc<Mutex<Vec<Arc<dyn Subscriber<State, Action> + Send + Sync>>>>,
    pub(crate) tx: Mutex<Option<SenderChannel<ActionOp<Action>>>>,
    // reduce and dispatch thread
    dispatcher: Mutex<Option<thread::JoinHandle<()>>>,
}

impl<State, Action> Default for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    fn default() -> Store<State, Action> {
        Store {
            state: Default::default(),
            reducers: Mutex::new(Vec::default()),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            tx: Mutex::new(None),
            dispatcher: Mutex::new(None),
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
        Self::new_with(reducer, state, name, DEFAULT_CAPACITY, DEFAULT_POLICY)
    }

    /// create a new store with name
    pub fn new_with(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
        state: State,
        name: String,
        capacity: usize,
        policy: channel::BackpressurePolicy,
    ) -> Result<Arc<Store<State, Action>>, StoreError> {
        // create a channel
        // and start a thread in which the store will listen for actions
        let (tx, rx) = BackpressureChannel::<ActionOp<Action>>::new(capacity, policy);
        let store = Store {
            state: Mutex::new(state),
            reducers: Mutex::new(vec![reducer]),
            subscribers: Arc::new(Mutex::new(Vec::default())),
            tx: Mutex::new(Some(tx)),
            dispatcher: Mutex::new(None),
        };

        // start a thread in which the store will listen for actions,
        // the shore referenced by Arc<Store> will be passed to the thread
        let rx_store = Arc::new(store);
        let tx_store = rx_store.clone();
        let builder = thread::Builder::new().name(name);
        let r = builder.spawn(move || {
            while let Some(action) = rx.recv() {
                match action {
                    ActionOp::Action(action) => {
                        if rx_store.do_reduce(&action) {
                            rx_store.do_notify(&action);
                        }
                    }
                    ActionOp::Exit => {
                        break;
                    }
                }
            }
        });

        let handle = r.map_err(|e| StoreError::Error(e.to_string()))?;
        tx_store.dispatcher.lock().unwrap().replace(handle);
        Ok(tx_store)
    }

    /// get the last state
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

    pub(crate) fn do_reduce(&self, action: &Action) -> bool {
        let mut state = self.state.lock().unwrap();
        let mut need_dispatch = false;
        for reducer in self.reducers.lock().unwrap().iter() {
            match reducer.reduce(&state, action) {
                DispatchOp::Dispatch(new_state) => {
                    *state = new_state;
                    need_dispatch = true;
                }
                DispatchOp::Keep(new_state) => {
                    // keep the state but do not dispatch
                    *state = new_state;
                }
            }
        }
        need_dispatch
    }

    pub(crate) fn do_notify(&self, action: &Action) {
        // TODO thread pool
        let subscribers = self.subscribers.lock().unwrap().clone();
        let state = self.state.lock().unwrap().clone();
        for subscriber in subscribers.iter() {
            subscriber.on_notify(&state, action);
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
        self.dispatcher.lock().unwrap().take()
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
        thunk: Box<dyn Fn(Arc<&dyn Dispatcher<Action>>) + Send>,
    ) -> thread::JoinHandle<()> {
        let self_clone = self.clone();
        thread::spawn(move || {
            let dispatcher = Arc::new(&self_clone as &dyn Dispatcher<Action>);
            thunk(dispatcher);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct TestReducer;

    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32> {
            DispatchOp::Dispatch(state + action)
        }
    }

    struct TestSubscriber;

    impl Subscriber<i32, i32> for TestSubscriber {
        fn on_notify(&self, state: &i32, action: &i32) {
            println!("State: {}, Action: {}", state, action);
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
        thread::sleep(Duration::from_millis(100)); // Wait for the dispatcher to process

        assert_eq!(store.get_state(), 1);
    }

    #[test]
    fn test_store_subscriber() {
        let reducer = Box::new(TestReducer);
        let store = Store::new(reducer);

        let subscriber = Arc::new(TestSubscriber);
        store.add_subscriber(subscriber);

        store.dispatch(1);
        thread::sleep(Duration::from_millis(100)); // Wait for the dispatcher to process

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
            reducer,
            10,
            "test_store".to_string(),
            5,
            channel::BackpressurePolicy::DropOldest,
        )
            .unwrap();

        assert_eq!(store.get_state(), 10);
    }
}
