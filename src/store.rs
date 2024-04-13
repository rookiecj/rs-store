use std::cell::RefCell;
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
    pub state: State,
    pub reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
    pub subscribers: RefCell<Vec<Box<dyn Subscriber<State, Action> + Send + Sync>>>,
    pub tx: Option<Sender<Action>>,
    dispatcher: Option<thread::JoinHandle<()>>,
}

impl<State, Action> Default for Store<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn default() -> Store<State, Action> {
        Store {
            state: Default::default(),
            reducers: Vec::default(),
            subscribers: RefCell::new(Vec::default()),
            tx: None,
            dispatcher: None,
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
    ) -> Arc<Mutex<Store<State, Action>>> {
        Self::new_with_state(reducer, Default::default())
    }

    pub fn new_with_state(
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
        state: State,
    ) -> Arc<Mutex<Store<State, Action>>> {
        // create a channel
        // and start a thread in which the store will listen for actions
        let (tx, rx) = std::sync::mpsc::channel::<Action>();
        let store = Store {
            state,
            reducers: vec![reducer],
            subscribers: RefCell::new(Vec::default()),
            tx: Some(tx),
            dispatcher: None,
        };

        // start a thread in which the store will listen for actions,
        // the shore referenced by Arc<Mutex<Store>> will be passed to the thread
        let rx_store = Arc::new(Mutex::new(store));
        let tx_store = rx_store.clone();
        let handle = thread::spawn(move || {
            for action in rx {
                {
                    let mut store = rx_store.lock().unwrap();
                    let state = store.do_reduce(&action);
                    store.do_notify(&state, &action);
                }
            }
        });

        tx_store.lock().unwrap().dispatcher = Some(handle);
        tx_store
    }

    pub fn wait_for(store: Arc<Mutex<Store<State, Action>>>) -> Result<(), StoreError> {
        // lock/unlock the store to get the handle
        let handle = store.lock().unwrap().take();
        // now safely join the handle without deadlock
        match handle {
            Some(handle) => handle.join().map_err(|_| StoreError::CloseError {
                inner: "join error".to_string(),
            }),
            None => Err(StoreError::CloseError {
                inner: "no handle".to_string(),
            }),
        }
    }

    pub fn add_reducer(&mut self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
        self.reducers.push(reducer);
    }

    pub fn add_subscriber(&mut self, subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>) {
        self.subscribers.get_mut().push(subscriber);
    }

    pub fn do_reduce(&mut self, action: &Action) -> State {
        for reducer in &self.reducers {
            self.state = reducer.reduce(&self.state, action);
        }
        self.state.clone()
    }

    pub fn do_notify(&mut self, state: &State, action: &Action) {
        for subscriber in self.subscribers.get_mut() {
            subscriber.notify(state, action);
        }
    }

    // pub fn dispatch(&self, action: Action) {
    //     if let Some(tx) = &self.tx {
    //         tx.send(action).unwrap_or(());
    //     }
    // }

    pub fn close(&mut self) {
        match self.tx.take() {
            Some(tx) => drop(tx),
            None => (),
        }
        // deadlock
        // match self.dispacher.take() {
        //     Some(handle) => handle.join().unwrap_or(()),
        //     None => (),
        // }
    }

    pub fn take(&mut self) -> Option<thread::JoinHandle<()>> {
        // deadlock
        return self.dispatcher.take();
    }
}

impl<State, Action> Dispatcher<Action> for Store<State, Action>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + 'static,
{
    fn dispatch(&self, action: Action) {
        if let Some(tx) = &self.tx {
            tx.send(action).unwrap_or(());
        }
    }
}

#[cfg(test)]
mod tests {}
