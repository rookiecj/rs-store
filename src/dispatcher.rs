use std::thread;

/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send> {
    /// dispatch is used to dispatch an action to the store
    fn dispatch(&self, action: Action);

    /// dispatch_thunk is used to dispatch a thunk.
    ///
    /// thunk is used to dispatch actions asynchronously
    /// thunk is a function that takes a dispatcher as an argument and dispatches actions to the store
    /// a thread is created to dispatch the thunk, so the thread is joined before the app exits
    fn dispatch_thunk(
        &self,
        thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>,
    ) -> thread::JoinHandle<()>;
}
