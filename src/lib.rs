use std::sync::mpsc::{Receiver, Sender};
use std::{sync, thread};
use std::thread::JoinHandle;


pub struct Store<State, Action, Reducer>
where
    State: Default + Clone + Send + 'static,
    Action: Clone + Send + 'static,
    Reducer: Fn(State, Action) -> State,
{
    state: State,
   // dispatcher: JoinHandle<()>,
    rx: Receiver<Action>,
    tx: Sender<Action>,
    reducers: Vec<Reducer>,
    //subscriber: Vec<Subscriber>,
}

impl<State, Action, Reducer> Store<State, Action, Reducer>
{
    fn new(state: State, reducer: Reducer) -> State {

        // dispatcher thread
        let (tx, rx) = sync::mpsc::channel::<Action>();
        let dispatch_thread = thread::spawn(|| {
           for action in rx {
               println!("action: {}", action);
           }
        });

        State {
            state,
            reducer,
            rx,
            tx,
        }
    }

    fn close(&self) {}
    pub fn dispatch(&self, action: Action) {}
    pub fn subscribe() {}


}

#[cfg(test)]
mod tests {
    use super::*;
}
