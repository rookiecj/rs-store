use crate::{DispatchOp, Dispatcher};

/// Reducer reduces the state based on the action.
pub trait Reducer<State, Action, Effect>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
    Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action, Effect>;
}

/// FnReducer is a reducer that is created from a function.
pub struct FnReducer<F, State, Action, Effect>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action, Effect>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
    Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync + 'static,
{
    func: F,
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action, Effect> Reducer<State, Action, Effect>
for FnReducer<F, State, Action, Effect>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action, Effect>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
    Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync + 'static,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State, Action, Effect> {
        (self.func)(state, action)
    }
}

impl<F, State, Action, Effect> From<F> for FnReducer<F, State, Action, Effect>
where
    F: Fn(&State, &Action) -> DispatchOp<State, Action, Effect>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync + 'static,
    Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync,
{
    fn from(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}
