/// Dispatcher dispatches actions to the store
pub trait Dispatcher<Action: Send + Sync> {
    fn dispatch(&self, action: Action);
}
