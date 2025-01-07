use crate::store::ActionOp;
use crate::Store;
use std::sync::Arc;

/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send + Clone> {
    /// dispatch is used to dispatch an action to the store
    /// the caller can be blocked by the channel
    fn dispatch(&self, action: Action);

    /// dispatch_thunk is used to dispatch a thunk.
    ///
    /// thunk is a function that takes a dispatcher as an argument and dispatches actions to the store
    fn dispatch_thunk(&self, thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>);

    /// dispatch_task is used to dispatch a task.
    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>);
}

impl<State, Action> Dispatcher<Action> for Arc<Store<State, Action>>
where
    State: Default + Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn dispatch(&self, action: Action) {
        let sender = self.tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            let _ = tx.send(ActionOp::Action(action)).unwrap_or(0);
        }
    }

    fn dispatch_thunk(&self, thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>) {
        let self_clone = self.clone();
        let dispatcher: Box<Arc<Store<State, Action>>> = Box::new(self_clone);
        match self.pool.lock() {
            Ok(pool) => {
                if let Some(pool) = pool.as_ref() {
                    pool.execute(move || {
                        thunk(dispatcher);
                    })
                }
            }
            Err(e) => {
                eprintln!("Failed to lock pool: {}", e);
            }
        }
    }

    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>) {
        match self.pool.lock() {
            Ok(pool) => {
                if let Some(pool) = pool.as_ref() {
                    pool.execute(move || {
                        task();
                    })
                }
            }
            Err(e) => {
                eprintln!("Failed to lock pool: {}", e);
            }
        }
    }
}
