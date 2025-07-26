/// Subscriber is a trait that can be implemented to receive notifications from the store.
pub trait Subscriber<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    /// on_notify is called when the store is notified of an action.
    fn on_notify(&self, state: &State, action: &Action);

    /// on_unsubscribe is called when a subscriber is unsubscribed from the store.
    fn on_unsubscribe(&self) {}
}

/// Subscription is a handle to unsubscribe from the store.
pub trait Subscription: Send {
    fn unsubscribe(&self);
}

/// FnSubscriber is a subscriber that is created from a function.
pub struct FnSubscriber<F, State, Action>
where
    F: Fn(&State, &Action),
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    func: F,
    // error[E0392]: parameter `State` is never used
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<F, State, Action> Subscriber<State, Action> for FnSubscriber<F, State, Action>
where
    F: Fn(&State, &Action),
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    fn on_notify(&self, state: &State, action: &Action) {
        (self.func)(state, action)
    }
}

impl<F, State, Action> From<F> for FnSubscriber<F, State, Action>
where
    F: Fn(&State, &Action),
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    fn from(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {

    #[allow(dead_code)]
    // 테스트용 상태 구조체
    #[derive(Default, Clone)]
    struct TestState {
        counter: i32,
        name: String,
    }

    // 테스트용 액션 enum
    #[allow(dead_code)]
    #[derive(Clone)]
    enum TestAction {
        IncrementCounter,
        #[allow(dead_code)]
        SetName(String),
    }
}
