use std::sync::mpsc::Sender;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreError {
    #[error("no error")]
    NoError,
    #[error("send error {inner}")]
    SendError {inner: String },
    #[error("lock error {inner}")]
    LockError {inner: String },
}


pub trait Reducer<State, Action>
    where
        State: Default + Send + Sync + Clone,
        Action: Send + Sync,
{
    fn reduce(&self, state: &State, action: &Action) -> State;

    /// <https://doc.rust-lang.org/book/ch10-02-traits.html>
    #[allow(clippy::all)]
    fn new() -> Box<dyn Reducer<State, Action> + Send + Sync>
        where
            Self: Default + Sized + Sync + Send + 'static,
    {
        Box::new(Self::default())
    }
}

pub trait Subscriber<State, Action>
    where
        State: Default + Send + Sync + Clone,
        Action: Send + Sync,
{
    fn notify(&self, state: &State, action: &Action);

    /// <https://doc.rust-lang.org/book/ch10-02-traits.html>
    #[allow(clippy::all)]
    fn new() -> Box<dyn Subscriber<State, Action> + Send + Sync>
        where
            Self: Default + Sized + Sync + Send + 'static,
    {
        Box::new(Self::default())
    }
}

pub struct Store<State, Action>
    where
        State: Default + Send + Sync + Clone,
        Action: Send + Sync,
{
    pub state: State,
    pub reducers: Vec<Box<dyn Reducer<State,Action> + Send + Sync>>,
    pub subscribers: Vec<Box<dyn Subscriber<State,Action> + Send + Sync>>,
    pub tx: Option<Sender<Action>>,
}

impl<State, Action> Default for Store<State,Action>
    where
        State: Default + Send + Sync + Clone,
        Action: Send + Sync,
{
    fn default() -> Store<State,Action> {
        Store {
            state: Default::default(),
            reducers: Vec::default(),
            subscribers: Vec::default(),
            tx: None,
        }
    }
}

impl<State, Action> Store<State, Action>
    where
        State: Default + Send + Sync + Clone,
        Action: Send + Sync,
{
    pub fn add_reducer(&mut self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) {
        self.reducers.push(reducer);
    }

    pub fn add_subscriber(&mut self, subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>) {
        self.subscribers.push(subscriber);
    }

    pub fn do_reduce(&mut self, action: &Action) -> State {
        for reducer in &self.reducers {
            self.state = reducer.reduce(&self.state, action);
        }
        self.state.clone()
    }

    pub fn do_notify(&mut self, state: &State, action: &Action) {
        for subscriber in &self.subscribers {
            subscriber.notify(state, action);
        }
    }

    pub fn dispatch(&self, action: Action) {
        if let Some(tx) = &self.tx {
            tx.send(action).unwrap_or(());
        }
    }

    pub fn close(&mut self) {
        match self.tx.take() {
            Some(tx) => drop(tx),
            None => (),
        }
    }
}

#[cfg(test)]
mod tests {}
