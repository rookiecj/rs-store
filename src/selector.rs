/// Selector is a function that selects a part of the state.
pub trait Selector<State, Output> {
    /// Selects a part of the state.
    fn select(&self, state: &State) -> Output;
}

/// FnSelector is a selector that is a function.
pub struct FnSelector<F, State, Output>
where
    F: Fn(&State) -> Output,
    State: Default + Send + Sync + Clone,
    Output: Send + Sync + Clone,
{
    func: Box<dyn Fn(&State) -> Output>,
    _marker: std::marker::PhantomData<F>,
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

impl<F, State, Output> From<F> for FnSelector<F, State, Output>
where
    F: Fn(&State) -> Output + 'static,
    State: Default + Send + Sync + Clone,
    Output: Send + Sync + Clone,
{
    fn from(func: F) -> Self {
        Self {
            func: Box::new(func),
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Clone)]
    struct State {
        a: i32,
    }

    #[test]
    fn test_selector() {
        let selector = FnSelector::from(|state: &State| state.a);
        let state = State { a: 1 };
        let output = selector.select(&state);
        assert_eq!(output, 1);
    }
}
