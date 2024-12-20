use crate::Dispatcher;

/// Represents a side effect that can be executed.
///
/// `Effect` is used to encapsulate actions that should be performed as a result of a state change.
/// These actions can be either simple functions or more complex thunks that require a dispatcher.
pub enum Effect<Action, State> {
    /// An action that should be dispatched.
    Action(Action),
    /// A task which will be executed asynchronously.
    Function(Box<dyn FnOnce() -> Result<State, String> + Send>),
    /// A task that takes the dispatcher as an argument.
    Thunk(Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>),
}

/// determine if the action should be dispatched or not
pub enum DispatchOp<State, Action> {
    /// Dispatch new state
    Dispatch(State, Option<Effect<Action, State>>),
    /// Keep new state but do not dispatch
    Keep(State, Option<Effect<Action, State>>),
}

/// Reducer reduces the state based on the action.
pub trait Reducer<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action>;
}

/// FnReducer is a reducer that is created from a function.
pub struct FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    func: F,
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action> Reducer<State, Action> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action> {
        (self.func)(state, action)
    }
}

impl<F, State, Action> From<F> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
{
    fn from(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}
