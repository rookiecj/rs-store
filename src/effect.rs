use crate::Dispatcher;

/// Represents a side effect that can be executed.
///
/// `Effect` is used to encapsulate actions that should be performed as a result of a state change.
/// an effect can be either simple function or more complex thunk that require a dispatcher.
pub enum Effect<Action> {
    /// An action that should be dispatched.
    Action(Action),
    /// A task which will be executed asynchronously.
    Task(Box<dyn FnOnce() + Send>),
    /// A task that takes the dispatcher as an argument.
    Thunk(Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>),
    /// A function which has a result.
    /// The result is an Any type which can be downcasted to the expected type,
    /// you should know the type and the String key can help.
    /// The result default ignored, if you want to get the result of the function, you can use `EffectMiddleware` to receive the result.
    Function(String, EffectFunction),
}

pub type EffectResult = Result<Box<dyn std::any::Any + Send>, Box<dyn std::error::Error + Send>>;
pub type EffectFunction = Box<dyn FnOnce() -> EffectResult + Send>;

/// EffectResultReceiver is a trait that can receive the result of an effect function.
pub trait EffectResultReceiver {
    fn receive(&self, key: String, result: EffectResult);
}
