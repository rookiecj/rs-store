use std::thread;

/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send> {
    /// dispatch is used to dispatch an action to the store
    /// the caller can be blocked by the channel
    fn dispatch(&self, action: Action);

    /// dispatch_thunk is used to dispatch a thunk.
    ///
    /// thunk is a function that takes a dispatcher as an argument and dispatches actions to the store
    /// a thread is created to dispatch the thunk, so the thread should be joined before the app exits.
    fn dispatch_thunk(
        &self,
        thunk: Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>,
    ) -> thread::JoinHandle<()>;

    /// dispatch_task is used to dispatch a task.
    fn dispatch_task(&self, task: Box<dyn FnOnce() + Send>) -> thread::JoinHandle<()>;
}
