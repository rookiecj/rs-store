/// Subscriber is a trait that can be implemented to receive notifications from the store.
pub trait Subscriber<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    /// on_subscribe is called when the subscriber is subscribed to the store.
    #[allow(unused_variables)]
    fn on_subscribe(&self, state: &State) {}

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
    use super::*;
    use std::sync::{Arc, Mutex};

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

    // 새로운 subscriber가 추가될 때 최신 상태를 받는지 테스트하는 subscriber
    struct TestSubscriberWithSubscribe {
        received_states: Arc<Mutex<Vec<TestState>>>,
        received_actions: Arc<Mutex<Vec<TestAction>>>,
        subscribe_called: Arc<Mutex<bool>>,
    }

    impl TestSubscriberWithSubscribe {
        fn new() -> Self {
            Self {
                received_states: Arc::new(Mutex::new(Vec::new())),
                received_actions: Arc::new(Mutex::new(Vec::new())),
                subscribe_called: Arc::new(Mutex::new(false)),
            }
        }

        fn get_received_states(&self) -> Vec<TestState> {
            self.received_states.lock().unwrap().clone()
        }

        fn get_received_actions(&self) -> Vec<TestAction> {
            self.received_actions.lock().unwrap().clone()
        }

        fn was_subscribe_called(&self) -> bool {
            *self.subscribe_called.lock().unwrap()
        }
    }

    impl Subscriber<TestState, TestAction> for TestSubscriberWithSubscribe {
        fn on_subscribe(&self, state: &TestState) {
            // 새로운 subscriber가 추가될 때 최신 상태를 받음
            self.received_states.lock().unwrap().push(state.clone());
            *self.subscribe_called.lock().unwrap() = true;
        }

        fn on_notify(&self, state: &TestState, action: &TestAction) {
            // 액션이 발생할 때마다 상태와 액션을 받음
            self.received_states.lock().unwrap().push(state.clone());
            self.received_actions.lock().unwrap().push(action.clone());
        }
    }

    #[test]
    fn test_subscriber_on_subscribe() {
        let subscriber = TestSubscriberWithSubscribe::new();
        let state = TestState {
            counter: 42,
            name: "test".to_string(),
        };

        // on_subscribe 호출
        subscriber.on_subscribe(&state);

        // 검증
        assert!(subscriber.was_subscribe_called());
        let received_states = subscriber.get_received_states();
        assert_eq!(received_states.len(), 1);
        assert_eq!(received_states[0].counter, 42);
        assert_eq!(received_states[0].name, "test");
    }

    #[test]
    fn test_subscriber_on_notify() {
        let subscriber = TestSubscriberWithSubscribe::new();
        let state = TestState {
            counter: 100,
            name: "notify".to_string(),
        };
        let action = TestAction::IncrementCounter;

        // on_notify 호출
        subscriber.on_notify(&state, &action);

        // 검증
        let received_states = subscriber.get_received_states();
        let received_actions = subscriber.get_received_actions();
        assert_eq!(received_states.len(), 1);
        assert_eq!(received_actions.len(), 1);
        assert_eq!(received_states[0].counter, 100);
        assert_eq!(received_states[0].counter, 100);
        assert_eq!(received_states[0].name, "notify");
    }

    // FnSubscriber 테스트
    #[test]
    fn test_fn_subscriber() {
        let received_states = Arc::new(Mutex::new(Vec::new()));
        let received_actions = Arc::new(Mutex::new(Vec::new()));

        let fn_subscriber = FnSubscriber::from(|state: &TestState, action: &TestAction| {
            received_states.lock().unwrap().push(state.clone());
            received_actions.lock().unwrap().push(action.clone());
        });

        let state = TestState {
            counter: 42,
            name: "test".to_string(),
        };
        let action = TestAction::IncrementCounter;

        // on_notify 호출
        fn_subscriber.on_notify(&state, &action);

        // 검증
        let states = received_states.lock().unwrap();
        let actions = received_actions.lock().unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(actions.len(), 1);
        assert_eq!(states[0].counter, 42);
        assert_eq!(states[0].name, "test");
    }

    // 여러 번의 on_subscribe 호출 테스트
    #[test]
    fn test_multiple_on_subscribe_calls() {
        let subscriber = TestSubscriberWithSubscribe::new();
        let state1 = TestState {
            counter: 10,
            name: "first".to_string(),
        };
        let state2 = TestState {
            counter: 20,
            name: "second".to_string(),
        };

        // 첫 번째 on_subscribe 호출
        subscriber.on_subscribe(&state1);
        assert!(subscriber.was_subscribe_called());
        assert_eq!(subscriber.get_received_states().len(), 1);
        assert_eq!(subscriber.get_received_states()[0].counter, 10);

        // 두 번째 on_subscribe 호출 (이미 subscribe_called가 true이므로 상태만 추가됨)
        subscriber.on_subscribe(&state2);
        assert!(subscriber.was_subscribe_called());
        assert_eq!(subscriber.get_received_states().len(), 2);
        assert_eq!(subscriber.get_received_states()[1].counter, 20);
    }

    // on_unsubscribe 기본 구현 테스트
    #[test]
    fn test_default_on_unsubscribe() {
        let subscriber = TestSubscriberWithSubscribe::new();

        // on_unsubscribe는 기본적으로 아무것도 하지 않음
        subscriber.on_unsubscribe();

        // subscriber의 상태가 변경되지 않았는지 확인
        assert!(!subscriber.was_subscribe_called());
        assert_eq!(subscriber.get_received_states().len(), 0);
        assert_eq!(subscriber.get_received_actions().len(), 0);
    }

    // 복잡한 상태와 액션을 사용한 테스트
    #[test]
    fn test_complex_state_and_action() {
        let subscriber = TestSubscriberWithSubscribe::new();

        // 복잡한 상태
        let complex_state = TestState {
            counter: 999,
            name: "complex_test_state".to_string(),
        };

        // 복잡한 액션
        let complex_action = TestAction::SetName("new_name".to_string());

        // on_subscribe 호출
        subscriber.on_subscribe(&complex_state);

        // on_notify 호출
        subscriber.on_notify(&complex_state, &complex_action);

        // 검증
        assert!(subscriber.was_subscribe_called());
        let states = subscriber.get_received_states();
        let actions = subscriber.get_received_actions();

        assert_eq!(states.len(), 2); // on_subscribe + on_notify
        assert_eq!(actions.len(), 1); // on_notify만

        assert_eq!(states[0].counter, 999);
        assert_eq!(states[0].name, "complex_test_state");
        assert_eq!(states[1].counter, 999);
        assert_eq!(states[1].name, "complex_test_state");

        match &actions[0] {
            TestAction::SetName(name) => assert_eq!(name, "new_name"),
            _ => panic!("Expected SetName action"),
        }
    }
}
