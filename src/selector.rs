/// Selector is a function that selects a part of the state.
pub trait Selector<State, Output> {
    fn select(&self, state: &State) -> Output;
}

/// FnSelector is a selector that is a function.
pub struct FnSelector<F, State, Output>
where
    F: Fn(&State) -> Output,
    State: Default + Send + Sync + Clone,
    Output: Send + Sync + Clone,
{
    func: F,
    _marker: std::marker::PhantomData<(State, Output)>,
}

impl<F, State, Output> Selector<State, Output> for FnSelector<F, State, Output>
where
    F: Fn(&State) -> Output,
    State: Default + Send + Sync + Clone,
    Output: Send + Sync + Clone,
{
    fn select(&self, state: &State) -> Output {
        (self.func)(state)
    }
}
