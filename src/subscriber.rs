use std::sync::Mutex;

use crate::Selector;

/// Subscriber is a trait that can be implemented to receive notifications from the store.
pub trait Subscriber<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
{
    /// on_notify is called when the store is notified of an action.
    fn on_notify(&self, state: &State, action: &Action);
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

/// SelectorSubscriber is a subscriber that has a selector.
/// It is used to subscribe to a specific part of the state.
/// when the state changes, the selector subscriber will notify the subscriber.
pub struct SelectorSubscriber<State, Action, Select, Output>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
    Select: Selector<State, Output>,
{
    selector: Select,
    last_value: Mutex<Option<Output>>,
    on_change: Box<dyn Fn(Output, Action) + Send + Sync>,
    _marker: std::marker::PhantomData<(State, Action)>,
}

impl<State, Action, Select, Output> SelectorSubscriber<State, Action, Select, Output>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
    Select: Selector<State, Output>,
    Output: PartialEq + Clone,
{
    pub fn new<F>(selector: Select, on_change: F) -> Self
    where
        F: Fn(Output, Action) + Send + Sync + 'static,
    {
        Self {
            selector,
            last_value: Mutex::new(None),
            on_change: Box::new(on_change),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<State, Action, Select, Output> Subscriber<State, Action>
    for SelectorSubscriber<State, Action, Select, Output>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone,
    Select: Selector<State, Output>,
    Output: PartialEq + Clone,
{
    fn on_notify(&self, state: &State, action: &Action) {
        let selected: Output = self.selector.select(state);
        let mut last_value = self.last_value.lock().unwrap();

        match last_value.as_ref() {
            Some(last) if *last == selected => {}
            _ => {
                (self.on_change)(selected.clone(), action.clone());
                *last_value = Some(selected);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    // 테스트용 상태 구조체
    #[derive(Default, Clone)]
    struct TestState {
        counter: i32,
        name: String,
    }

    // 테스트용 액션 enum
    #[derive(Clone)]
    enum TestAction {
        IncrementCounter,
        #[allow(dead_code)]
        SetName(String),
    }

    // 카운터 값만 선택하는 셀렉터
    struct CounterSelector;
    impl Selector<TestState, i32> for CounterSelector {
        fn select(&self, state: &TestState) -> i32 {
            state.counter
        }
    }

    // 이름만 선택하는 셀렉터
    struct NameSelector;
    impl Selector<TestState, String> for NameSelector {
        fn select(&self, state: &TestState) -> String {
            state.name.clone()
        }
    }

    #[test]
    fn test_selector_subscriber_counter() {
        // 변경 횟수를 추적하기 위한 카운터
        let change_counter = Arc::new(AtomicI32::new(0));
        let change_counter_clone = change_counter.clone();

        // SelectorSubscriber 생성
        let subscriber =
            SelectorSubscriber::new(CounterSelector, move |_value: i32, _action: TestAction| {
                change_counter_clone.fetch_add(1, Ordering::SeqCst);
            });

        // 초기 상태
        let initial_state = TestState {
            counter: 0,
            name: "initial".to_string(),
        };

        subscriber.on_notify(&initial_state, &TestAction::IncrementCounter);
        assert_eq!(change_counter.load(Ordering::SeqCst), 1);

        // 상태가 변경된 경우
        let new_state = TestState {
            counter: 1,
            name: "initial".to_string(),
        };
        subscriber.on_notify(&new_state, &TestAction::IncrementCounter);
        assert_eq!(change_counter.load(Ordering::SeqCst), 2);

        // 동일한 값으로 변경된 경우 (콜백이 호출되지 않아야 함)
        subscriber.on_notify(&new_state, &TestAction::IncrementCounter);
        assert_eq!(change_counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_selector_subscriber_name() {
        let received_values = Arc::new(Mutex::new(Vec::new()));
        let received_values_clone = received_values.clone();

        // 이름 변경을 구독하는 SelectorSubscriber
        let subscriber =
            SelectorSubscriber::new(NameSelector, move |value: String, _action: TestAction| {
                received_values_clone.lock().unwrap().push(value);
            });

        // 초기 상태
        let initial_state = TestState {
            counter: 0,
            name: "initial".to_string(),
        };

        // 이름이 변경된 상태
        let new_state = TestState {
            counter: 0,
            name: "updated".to_string(),
        };

        // 상태 변경 알림
        subscriber.on_notify(&initial_state, &TestAction::SetName("initial".to_string()));
        subscriber.on_notify(&new_state, &TestAction::SetName("updated".to_string()));
        subscriber.on_notify(&new_state, &TestAction::SetName("updated".to_string())); // 동일한 값

        // 결과 확인
        let received = received_values.lock().unwrap();
        assert_eq!(received.len(), 2); // 고유한 값 변경은 2번
        assert_eq!(received[0], "initial");
        assert_eq!(received[1], "updated");
    }

    #[test]
    fn test_selector_subscriber_multiple_fields() {
        // 두 필드의 변경을 모두 추적
        let counter_changes = Arc::new(AtomicI32::new(0));
        let name_changes = Arc::new(Mutex::new(Vec::new()));

        let counter_changes_clone = counter_changes.clone();
        let name_changes_clone = name_changes.clone();

        // 카운터 구독자
        let counter_subscriber =
            SelectorSubscriber::new(CounterSelector, move |_value: i32, _action: TestAction| {
                counter_changes_clone.fetch_add(1, Ordering::SeqCst);
            });

        // 이름 구독자
        let name_subscriber =
            SelectorSubscriber::new(NameSelector, move |value: String, _action: TestAction| {
                name_changes_clone.lock().unwrap().push(value);
            });

        // 상태 변경 시나리오
        let states = vec![
            TestState {
                counter: 0,
                name: "initial".to_string(),
            },
            TestState {
                counter: 1,
                name: "initial".to_string(),
            },
            TestState {
                counter: 1,
                name: "updated".to_string(),
            },
        ];

        // 각 상태 변경에 대해 두 구독자 모두에게 알림
        for state in states {
            counter_subscriber.on_notify(&state, &TestAction::IncrementCounter);
            name_subscriber.on_notify(&state, &TestAction::SetName(state.name.clone()));
        }

        // 결과 확인
        assert_eq!(counter_changes.load(Ordering::SeqCst), 2); // 카운터는 2번 변경됨
        let received_names = name_changes.lock().unwrap();
        assert_eq!(received_names.len(), 2); // 이름은 2번 변경됨
        assert_eq!(received_names[0], "initial");
        assert_eq!(received_names[1], "updated");
    }
}
