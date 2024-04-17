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
    fn notify(&mut self, state: &State, action: &Action);
}

pub trait Dispatcher<Action: Send + Sync> {
    fn dispatch(&self, action: Action);
}

pub struct Store<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    //pub state: Mutex<State>,
    pub reducers: Mutex<Vec<Box<dyn Reducer<State, Action> + Send + Sync>>>,
    pub subscribers: Mutex<Vec<Box<dyn Subscriber<State, Action> + Send + Sync>>>,
    pub tx: Mutex<Option<Sender<Action>>>,
    dispatcher: Mutex<Option<thread::JoinHandle<()>>>,
}

impl<State, Action> Default for Store<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn default() -> Store<State, Action> {
        Store {
            //state: Default::default(),
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
    pub fn new(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
    ) -> Arc<Store<State, Action>> {
        Self::new_with_state(reducer, Default::default())
    }

    pub fn new_with_state(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
        mut state: State,
    ) -> Arc<Store<State, Action>> {
        // create a channel
        // and start a thread in which the store will listen for actions
        let (tx, rx) = std::sync::mpsc::channel::<Action>();
        let store = Store {
            //state: Mutex::new(state),
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
                rx_store.do_reduce(&mut state, &action);
                rx_store.do_notify(&state, &action);
            }
        });

        tx_store.dispatcher.lock().unwrap().replace(handle);
        tx_store
    }

    pub fn add_reducer(&self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
        self.reducers.lock().unwrap().push(reducer);
    }

    pub fn add_subscriber(&self, subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>) {
        self.subscribers.lock().unwrap().push(subscriber);
    }

    pub fn do_reduce(&self, state: &mut State, action: &Action) {
        for reducer in self.reducers.lock().unwrap().iter() {
            *state = reducer.reduce(&state, action);
        }
    }

    pub fn do_notify(&self, state: &State, action: &Action) {
        for subscriber in self.subscribers.lock().unwrap().iter_mut() {
            subscriber.notify(state, action);
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

        let handle = self.detach();
        if let Some(handle) = handle {
            handle.join().unwrap_or(());
        }
    }

    /// detach the dispatcher thread from the store
    pub fn detach(&self) -> Option<thread::JoinHandle<()>> {
        self.dispatcher.lock().unwrap().take()
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
