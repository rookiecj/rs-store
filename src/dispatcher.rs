use crate::store_impl::ActionOp;
use crate::{StoreError, StoreImpl};
use std::sync::Arc;

/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send + Clone> {
    /// dispatch is used to dispatch an action to the store
    /// the caller can be blocked by the channel
    fn dispatch(&self, action: Action) -> Result<(), StoreError>;

    /// dispatch_thunk is used to dispatch a thunk.
    ///
    /// thunk is a function that takes a dispatcher as an argument and dispatches actions to the store
    fn dispatch_thunk(&self, thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>);

    /// dispatch_task is used to dispatch a task.
    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>);
}

impl<State, Action> Dispatcher<Action> for Arc<StoreImpl<State, Action>>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        let sender = self.dispatch_tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            match tx.send(ActionOp::Action(action)) {
                Ok(_) => Ok(()),
                Err(_e) => {
                    // eprintln!("Failed to send action: {}", e);
                    Err(StoreError::DispatchError(
                        "Failed to send action".to_string(),
                    ))
                }
            }
        } else {
            Err(StoreError::DispatchError("Store is stopped".to_string()))
        }
    }

    fn dispatch_thunk(&self, thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>) {
        let self_clone = self.clone();
        let dispatcher: Box<Arc<StoreImpl<State, Action>>> = Box::new(self_clone);
        // poll can be shutdown already
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
