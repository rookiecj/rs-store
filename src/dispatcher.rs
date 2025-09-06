use crate::store_impl::ActionOp;
use crate::{StoreError, StoreImpl};
use std::sync::{Arc, Weak};

/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send + Clone + std::fmt::Debug>: Send {
    /// dispatch is used to dispatch an action to the store
    /// the action can be dropped if the store is full
    fn dispatch(&self, action: Action) -> Result<(), StoreError>;

    /// dispatch_thunk is used to dispatch a thunk.
    ///
    /// thunk is a function that takes a dispatcher as an argument and dispatches actions to the store
    fn dispatch_thunk(&self, thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>);

    /// dispatch_task is used to dispatch a task.
    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>);
}

/// WeakDispatcher is a dispatcher that holds a weak reference to the store
/// to prevent circular reference
pub(crate) struct WeakDispatcher<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    store: Weak<StoreImpl<State, Action>>,
}

// WeakDispatcher는 Send + Sync를 구현합니다
unsafe impl<State, Action> Send for WeakDispatcher<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
}

unsafe impl<State, Action> Sync for WeakDispatcher<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
}

impl<State, Action> WeakDispatcher<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    /// WeakDispatcher 생성자
    pub fn new(store: Arc<StoreImpl<State, Action>>) -> Self {
        Self {
            store: Arc::downgrade(&store),
        }
    }
}

impl<State, Action> Dispatcher<Action> for WeakDispatcher<State, Action>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        // weak reference를 strong reference로 업그레이드 시도
        if let Some(store) = self.store.upgrade() {
            let sender = store.dispatch_tx.lock().unwrap();
            if let Some(tx) = sender.as_ref() {
                match tx.send(ActionOp::Action(action)) {
                    Ok(_) => Ok(()),
                    Err(_e) => {
                        #[cfg(feature = "store-log")]
                        eprintln!("Failed to send action: {:?}", _e);
                        Err(StoreError::DispatchError(
                            "Failed to send action".to_string(),
                        ))
                    }
                }
            } else {
                Err(StoreError::DispatchError("Store is stopped".to_string()))
            }
        } else {
            // store가 이미 drop된 경우
            Err(StoreError::DispatchError(
                "Store has been dropped".to_string(),
            ))
        }
    }

    fn dispatch_thunk(&self, thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>) {
        if let Some(store) = self.store.upgrade() {
            // WeakDispatcher를 복제하여 thunk에 전달
            let weak_dispatcher = WeakDispatcher::new(store.clone());
            let dispatcher: Box<dyn Dispatcher<Action>> = Box::new(weak_dispatcher);

            match store.pool.lock() {
                Ok(pool) => {
                    if let Some(pool) = pool.as_ref() {
                        pool.execute(move || {
                            thunk(dispatcher);
                        })
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "store-log")]
                    eprintln!("Failed to lock pool: {}", _e);
                }
            }
        } else {
            #[cfg(feature = "store-log")]
            eprintln!("Store has been dropped, cannot dispatch thunk");
        }
    }

    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>) {
        if let Some(store) = self.store.upgrade() {
            match store.pool.lock() {
                Ok(pool) => {
                    if let Some(pool) = pool.as_ref() {
                        pool.execute(move || {
                            task();
                        })
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "store-log")]
                    eprintln!("Failed to lock pool: {}", _e);
                }
            }
        } else {
            #[cfg(feature = "store-log")]
            eprintln!("Store has been dropped, cannot dispatch task");
        }
    }
}

impl<State, Action> Dispatcher<Action> for Arc<StoreImpl<State, Action>>
where
    State: Send + Sync + Clone + std::fmt::Debug + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn dispatch(&self, action: Action) -> Result<(), StoreError> {
        let sender = self.dispatch_tx.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            match tx.send(ActionOp::Action(action)) {
                Ok(_) => Ok(()),
                Err(_e) => {
                    #[cfg(feature = "store-log")]
                    eprintln!("Failed to send action: {:?}", _e);
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
        let weak_dispatcher = WeakDispatcher::new(self.clone());
        let dispatcher: Box<dyn Dispatcher<Action>> = Box::new(weak_dispatcher);
        // poll can be shutdown already
        match self.pool.lock() {
            Ok(pool) => {
                if let Some(pool) = pool.as_ref() {
                    pool.execute(move || {
                        thunk(dispatcher);
                    })
                }
            }
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("Failed to lock pool: {}", _e);
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
            Err(_e) => {
                #[cfg(feature = "store-log")]
                eprintln!("Failed to lock pool: {}", _e);
            }
        }
    }
}
