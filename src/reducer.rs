use crate::{DispatchOp, Reducer};

pub struct FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    func: F,
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action> Reducer<State, Action> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn reduce(&self, state: &State, action: &Action) -> DispatchOp<State> {
        (self.func)(state, action)
    }
}

impl<F, State, Action> From<F> for FnReducer<F, State, Action>
where
    F: Fn(&State, &Action) -> DispatchOp<State>,
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn from(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}
