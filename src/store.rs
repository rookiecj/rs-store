use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreError {
    #[error("no error")]
    NoError,
    #[error("send error {inner}")]
    SendError { inner: String },
    #[error("lock error {inner}")]
    LockError { inner: String },
    #[error("close error {inner}")]
    CloseError { inner: String },
}

pub trait Reducer<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn reduce(&self, state: &State, action: &Action) -> State;
}

pub trait Subscriber<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn on_notify(&self, state: &State, action: &Action);
}

/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send + Sync> {
    fn dispatch(&self, action: Action);
}

/// Store is a simple implementation of a Redux store
pub struct Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    state: Mutex<State>,
    pub reducers: Mutex<Vec<Box<dyn Reducer<State, Action> + Send + Sync>>>,
    pub subscribers: Mutex<Vec<Arc<dyn Subscriber<State, Action> + Send + Sync>>>,
    pub tx: Mutex<Option<Sender<Action>>>,
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
            subscribers: Mutex::new(Vec::default()),
            tx: Mutex::new(None),
            dispatcher: Mutex::new(None),
        }
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
        // create a channel
        // and start a thread in which the store will listen for actions
        let (tx, rx) = std::sync::mpsc::channel::<Action>();
        let store = Store {
            state: Mutex::new(state),
            reducers: Mutex::new(vec![reducer]),
            subscribers: Mutex::new(Vec::default()),
            tx: Mutex::new(Some(tx)),
            dispatcher: Mutex::new(None),
        };

        // start a thread in which the store will listen for actions,
        // the shore referenced by Arc<Store> will be passed to the thread
        let rx_store = Arc::new(store);
        let tx_store = rx_store.clone();
        let handle = thread::spawn(move || {
            for action in rx {
                rx_store.do_reduce(&action);
                rx_store.do_notify(&action);
            }
        });

        tx_store.dispatcher.lock().unwrap().replace(handle);
        tx_store
    }

    /// get last state
    pub fn get_state(&self) -> State {
        return self.state.lock().unwrap().clone();
    }

    /// add a reducer to the store
    pub fn add_reducer(&self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
        self.reducers.lock().unwrap().push(reducer);
    }

    /// add a subscriber to the store
    pub fn add_subscriber(&self, subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>) {
        self.subscribers.lock().unwrap().push(subscriber);
    }

    pub(crate) fn do_reduce(&self, action: &Action) {
        let mut state = self.state.lock().unwrap();
        for reducer in self.reducers.lock().unwrap().iter() {
            *state = reducer.reduce(&state, action);
        }
    }

    pub(crate) fn do_notify(&self, action: &Action) {
        // TODO thread pool
        if let subscribers = self.subscribers.lock().unwrap().clone() {
            let state = self.state.lock().unwrap().clone();
            for subscriber in subscribers.iter() {
                subscriber.on_notify(&state, action);
            }
        }
    }

    /// close the store
    pub fn close(&self) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
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

impl<State, Action> Dispatcher<Action> for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    fn dispatch(&self, action: Action) {
        let sender = self.tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            tx.send(action).unwrap_or(());
        }
    }
}

#[cfg(test)]
mod tests {}
