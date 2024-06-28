pub trait Subscriber<State, Action>
where
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn on_notify(&self, state: &State, action: &Action);
}

/// it is used for deregistration
pub trait Subscription: Send {
    fn unsubscribe(&self);
}

pub struct FnSubscriber<F, State, Action>
where
    F: Fn(&State, &Action),
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    func: F,
    // error[E0392]: parameter `State` is never used
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action> Subscriber<State, Action> for FnSubscriber<F, State, Action>
where
    F: Fn(&State, &Action),
    State: Default + Send + Sync + Clone,
    Action: Send + Sync,
{
    fn on_notify(&self, state: &State, action: &Action) {
        (self.func)(state, action)
    }
}

impl<F, State, Action> From<F> for FnSubscriber<F, State, Action>
where
    F: Fn(&State, &Action),
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
