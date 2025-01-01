use crate::effect::Effect;

/// determine if the action should be dispatched or not
pub enum DispatchOp<State, Action> {
    /// Dispatch new state
    Dispatch(State, Option<Effect<Action>>),
    /// Keep new state but do not dispatch
    Keep(State, Option<Effect<Action>>),
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
