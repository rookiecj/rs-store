use crate::{BackpressurePolicy, Subscriber, Subscription};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default capacity for the channel
pub const DEFAULT_CAPACITY: usize = 16;
pub const DEFAULT_STORE_NAME: &str = "store";

/// StoreError represents an error that occurred in the store
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("dispatch error: {0}")]
    DispatchError(String),
    #[error("reducer error: {0}")]
    ReducerError(String),
    #[error("subscription error: {0}")]
    SubscriptionError(String),
    #[error("middleware error: {0}")]
    MiddlewareError(String),
    #[error("initialization error: {0}")]
    InitError(String),
    /// state update failed with context and source
    #[error("state update failed: {context}, cause: {source}")]
    StateUpdateError {
        context: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Store is a thread-safe and concurrent-safe store for Redux-like state management
/// It provides a way to manage the state of an application in a thread-safe and concurrent-safe manner
/// It supports reducers, subscribers, and async actions through Thunk
pub trait Store<State, Action>: Send + Sync
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    /// Get the current state
    fn get_state(&self) -> State;

    /// Dispatch an action
    fn dispatch(&self, action: Action) -> Result<(), StoreError>;

    /// Add a subscriber to the store
    /// store updates are delivered to the subscriber in same reducer thread
    fn add_subscriber(
        &self,
        subscriber: Arc<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError>;

    ///// Iterate over the store's state and action pairs
    //fn iter(&self) -> impl Iterator<Item = (State, Action)>;

    /// subscribe to the store in new context
    /// store updates are delivered to the subscriber in the new context
    fn subscribed(
        &self,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError>;

    /// subscribe to the store in new context
    /// store updates are delivered to the subscriber in the new context
    ///
    /// ### Parameters
    /// * capacity: Channel buffer capacity
    /// * policy: Backpressure policy for when down channel(store to subscriber) is full
    ///
    /// ### Return
    /// * Subscription: Subscription for the store,
    fn subscribed_with(
        &self,
        capacity: usize,
        policy: BackpressurePolicy<(Instant, State, Action)>,
        subscriber: Box<dyn Subscriber<State, Action> + Send + Sync>,
    ) -> Result<Box<dyn Subscription>, StoreError>;

    /// Stop the store
    /// when the queue is full, the send can be blocked if there is no droppable item
    fn stop(&self) -> Result<(), StoreError>;

    /// Stop the store with timeout
    fn stop_timeout(&self, timeout: Duration) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::StoreBuilder;
    use crate::BackpressurePolicy;
    use crate::Reducer;
    use crate::StoreImpl;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    // Mock implementations for testing
    #[derive(Debug, Clone, PartialEq)]
    struct TestState {
        counter: i32,
        message: String,
    }

    impl Default for TestState {
        fn default() -> Self {
            TestState {
                counter: 0,
                message: String::new(),
            }
        }
    }

    #[derive(Debug, Clone)]
    enum TestAction {
        Increment,
        Decrement,
        SetMessage(String),
    }

    struct TestReducer;

    impl Reducer<TestState, TestAction> for TestReducer {
        fn reduce(&self, state: &TestState, action: &TestAction) -> crate::DispatchOp<TestState, TestAction> {
            match action {
                TestAction::Increment => {
                    let mut new_state = state.clone();
                    new_state.counter += 1;
                    crate::DispatchOp::Dispatch(new_state, vec![])
                }
                TestAction::Decrement => {
                    let mut new_state = state.clone();
                    new_state.counter -= 1;
                    crate::DispatchOp::Dispatch(new_state, vec![])
                }
                TestAction::SetMessage(msg) => {
                    let mut new_state = state.clone();
                    new_state.message = msg.clone();
                    crate::DispatchOp::Dispatch(new_state, vec![])
                }
            }
        }
    }

    fn create_test_store() -> Arc<StoreImpl<TestState, TestAction>> {
        StoreImpl::new_with(
            TestState::default(),
            vec![Box::new(TestReducer)],
            "test-store".into(),
            16,
            BackpressurePolicy::default(),
            vec![],
        )
        .unwrap()
    }

    struct TestChannneledReducer;

    impl Reducer<i32, i32> for TestChannneledReducer {
        fn reduce(&self, state: &i32, action: &i32) -> crate::DispatchOp<i32, i32> {
            crate::DispatchOp::Dispatch(state + action, vec![])
        }
    }

    struct TestChannelSubscriber {
        received: Arc<Mutex<Vec<(i32, i32)>>>,
    }

    impl TestChannelSubscriber {
        fn new(received: Arc<Mutex<Vec<(i32, i32)>>>) -> Self {
            Self { received }
        }
    }

    impl Subscriber<i32, i32> for TestChannelSubscriber {
        fn on_notify(&self, state: i32, action: i32) {
            //println!("TestChannelSubscriber: state={}, action={}", state, action);
            self.received.lock().unwrap().push((state, action));
        }
    }

    struct SlowSubscriber {
        received: Arc<Mutex<Vec<(i32, i32)>>>,
        delay: Duration,
    }

    impl SlowSubscriber {
        fn new(received: Arc<Mutex<Vec<(i32, i32)>>>, delay: Duration) -> Self {
            Self { received, delay }
        }
    }

    impl Subscriber<i32, i32> for SlowSubscriber {
        fn on_notify(&self, state: i32, action: i32) {
            //println!("SlowSubscriber: state={}, action={}", state, action);
            std::thread::sleep(self.delay);
            self.received.lock().unwrap().push((state, action));
        }
    }

    #[test]
    fn test_store_get_state() {
        let store = create_test_store();
        let initial_state = store.get_state();
        assert_eq!(initial_state.counter, 0);
        assert_eq!(initial_state.message, "");
    }

    #[test]
    fn test_store_dispatch() {
        let store = create_test_store();

        // Dispatch increment action
        store.dispatch(TestAction::Increment).unwrap();
        thread::sleep(Duration::from_millis(50)); // Wait for async processing

        let state = store.get_state();
        assert_eq!(state.counter, 1);

        // Dispatch set message action
        store.dispatch(TestAction::SetMessage("Hello".into())).unwrap();
        thread::sleep(Duration::from_millis(50));

        let state = store.get_state();
        assert_eq!(state.message, "Hello");

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_multiple_actions() {
        let store = create_test_store();

        // Dispatch multiple actions
        store.dispatch(TestAction::Increment).unwrap();
        store.dispatch(TestAction::Increment).unwrap();
        store.dispatch(TestAction::SetMessage("Test".into())).unwrap();
        store.dispatch(TestAction::Decrement).unwrap();

        thread::sleep(Duration::from_millis(100));

        let final_state = store.get_state();
        assert_eq!(final_state.counter, 1);
        assert_eq!(final_state.message, "Test");

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_after_stop() {
        let store = create_test_store();
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // Dispatch should fail after stop
        let result = store.dispatch(TestAction::Increment);
        assert!(result.is_err());

        match result {
            Err(StoreError::DispatchError(_)) => (),
            _ => panic!("Expected DispatchError"),
        }
    }

    #[test]
    fn test_store_concurrent_access() {
        let store = Arc::new(create_test_store());
        let store_clone = store.clone();

        let handle = thread::spawn(move || {
            for _ in 0..5 {
                store_clone.dispatch(TestAction::Increment).unwrap();
                thread::sleep(Duration::from_millis(10));
            }
        });

        for _ in 0..5 {
            store.dispatch(TestAction::Decrement).unwrap();
            thread::sleep(Duration::from_millis(10));
        }

        handle.join().unwrap();
        thread::sleep(Duration::from_millis(100));

        let final_state = store.get_state();
        // Final counter should be 0 (5 increments and 5 decrements)
        assert_eq!(final_state.counter, 0);

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_builder_configurations() {
        #[allow(deprecated)]
        let store = StoreBuilder::new(TestState::default())
            .with_reducer(Box::new(TestReducer))
            .with_name("custom-store".into())
            .with_capacity(32)
            .with_policy(BackpressurePolicy::DropLatestIf(None))
            .build()
            .unwrap();

        store.dispatch(TestAction::Increment).unwrap();
        thread::sleep(Duration::from_millis(50));

        let state = store.get_state();
        assert_eq!(state.counter, 1);

        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }
    }

    #[test]
    fn test_store_error_handling() {
        let store = create_test_store();
        match store.stop() {
            Ok(_) => println!("store stopped"),
            Err(e) => {
                panic!("store stop failed  : {:?}", e);
            }
        }

        // Test various error conditions
        let dispatch_result = store.dispatch(TestAction::Increment);
        // println!("dispatch_result: {:?}", dispatch_result);
        // dispatch_result: Err(DispatchError("Dispatch channel is closed"))
        assert!(matches!(dispatch_result, Err(StoreError::DispatchError(_))));

        // Test that the store remains in a consistent state after errors
        let state = store.get_state();
        assert_eq!(state.counter, 0);
    }

    // subscribed() 기본 기능 테스트 - 별도 스레드에서 실행되는지 확인
    #[test]
    fn test_subscribed_basic_functionality() {
        let store =
            StoreBuilder::new_with_reducer(0, Box::new(TestChannneledReducer)).build().unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Box::new(TestChannelSubscriber::new(received.clone()));

        // subscribed()로 구독 - 별도 스레드에서 실행됨
        let subscription = store.subscribed(subscriber).unwrap();

        // 액션들을 dispatch
        store.dispatch(1).unwrap();
        store.dispatch(2).unwrap();
        store.dispatch(3).unwrap();

        // 잠시 대기하여 채널을 통해 메시지가 전달되도록 함
        thread::sleep(Duration::from_millis(100));

        // store 정지
        store.stop().unwrap();

        // 구독 해제
        subscription.unsubscribe();

        // 수신된 메시지 검증
        let states = received.lock().unwrap();
        assert_eq!(states.len(), 3);
        assert_eq!(states[0], (1, 1)); // state=0+1, action=1
        assert_eq!(states[1], (3, 2)); // state=1+2, action=2
        assert_eq!(states[2], (6, 3)); // state=3+3, action=3
    }

    // subscribed() 동시성 테스트 - 여러 subscriber가 동시에 실행되는지 확인
    #[test]
    fn test_subscribed_concurrent_subscribers() {
        let store =
            StoreBuilder::new_with_reducer(0, Box::new(TestChannneledReducer)).build().unwrap();

        let received1 = Arc::new(Mutex::new(Vec::new()));
        let received2 = Arc::new(Mutex::new(Vec::new()));
        let received3 = Arc::new(Mutex::new(Vec::new()));

        // 3개의 subscriber를 각각 별도 스레드에서 실행
        let subscription1 =
            store.subscribed(Box::new(TestChannelSubscriber::new(received1.clone()))).unwrap();
        let subscription2 =
            store.subscribed(Box::new(TestChannelSubscriber::new(received2.clone()))).unwrap();
        let subscription3 =
            store.subscribed(Box::new(TestChannelSubscriber::new(received3.clone()))).unwrap();

        // 동시에 여러 액션을 dispatch
        for i in 1..=10 {
            store.dispatch(i).unwrap();
        }

        // 잠시 대기
        thread::sleep(Duration::from_millis(200));

        // store 정지
        store.stop().unwrap();

        // 모든 구독 해제
        subscription1.unsubscribe();
        subscription2.unsubscribe();
        subscription3.unsubscribe();

        // 모든 subscriber가 동일한 메시지를 받았는지 확인
        let states1 = received1.lock().unwrap();
        let states2 = received2.lock().unwrap();
        let states3 = received3.lock().unwrap();

        assert_eq!(states1.len(), 10);
        assert_eq!(states2.len(), 10);
        assert_eq!(states3.len(), 10);

        // 각 subscriber가 동일한 순서로 메시지를 받았는지 확인
        for i in 0..10 {
            assert_eq!(states1[i].1, states2[i].1); // action이 동일
            assert_eq!(states2[i].1, states3[i].1);
        }
    }

    // subscribed() 백프레셔 정책 테스트 - DropLatestIf 정책
    #[test]
    fn test_subscribed_drop_latest_if_policy() {
        let store =
            StoreBuilder::new_with_reducer(0, Box::new(TestChannneledReducer)).build().unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Box::new(SlowSubscriber::new(
            received.clone(),
            Duration::from_millis(50),
        ));

        // 작은 용량과 DropLatestIf 정책으로 구독
        let predicate = Box::new(|(_, _, action): &(Instant, i32, i32)| *action < 5);
        let policy = BackpressurePolicy::DropLatestIf(Some(predicate));
        let subscription = store.subscribed_with(2, policy, subscriber).unwrap();

        // 채널을 가득 채우는 액션들을 빠르게 dispatch
        for i in 1..=10 {
            store.dispatch(i).unwrap();
        }

        // 처리 시간 대기
        thread::sleep(Duration::from_millis(300));

        store.stop().unwrap();
        subscription.unsubscribe();

        // 백프레셔 정책에 의해 일부 메시지가 드롭되었는지 확인
        let states = received.lock().unwrap();
        // 채널 용량이 2이고 SlowSubscriber가 느리므로 일부 메시지만 처리됨
        assert!(
            states.len() < 10,
            "Expected fewer than 10 messages due to backpressure, got {}",
            states.len()
        );

        // 최소한 일부 메시지는 받았는지 확인
        assert!(
            states.len() > 0,
            "Expected at least some messages to be received"
        );
    }

    // subscribed() 에러 처리 테스트 - 잘못된 capacity로 구독 시도
    #[test]
    fn test_subscribed_error_handling() {
        let store =
            StoreBuilder::new_with_reducer(0, Box::new(TestChannneledReducer)).build().unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Box::new(TestChannelSubscriber::new(received.clone()));

        // 유효한 구독
        let subscription = store.subscribed(subscriber).unwrap();
        // subscription은 Box<dyn Subscription>이므로 is_some() 메서드가 없음

        // 구독 해제
        subscription.unsubscribe();

        // 이미 해제된 구독에 대한 추가 액션은 처리되지 않아야 함
        store.dispatch(1).unwrap();
        thread::sleep(Duration::from_millis(50));

        let states = received.lock().unwrap();
        assert_eq!(states.len(), 0); // 해제 후에는 메시지를 받지 않아야 함

        store.stop().unwrap();
    }

    // subscribed() 스레드 생명주기 테스트
    #[test]
    fn test_subscribed_thread_lifecycle() {
        let store =
            StoreBuilder::new_with_reducer(0, Box::new(TestChannneledReducer)).build().unwrap();

        let received = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Box::new(TestChannelSubscriber::new(received.clone()));

        // 구독 생성
        let subscription = store.subscribed(subscriber).unwrap();

        // 액션 dispatch
        store.dispatch(1).unwrap();
        thread::sleep(Duration::from_millis(50));

        // 구독 해제 - 스레드가 정상적으로 종료되어야 함
        subscription.unsubscribe();

        // 추가 액션은 처리되지 않아야 함
        store.dispatch(2).unwrap();
        thread::sleep(Duration::from_millis(50));

        let states = received.lock().unwrap();
        assert_eq!(states.len(), 1); // 첫 번째 액션만 처리됨
        assert_eq!(states[0], (1, 1));

        store.stop().unwrap();
    }

    // subscribed()와 add_subscriber() 혼합 사용 테스트
    #[test]
    fn test_subscribed_mixed_with_add_subscriber() {
        let store =
            StoreBuilder::new_with_reducer(0, Box::new(TestChannneledReducer)).build().unwrap();

        // 일반 subscriber (reducer 스레드에서 실행)
        let received_main = Arc::new(Mutex::new(Vec::new()));
        let subscriber_main = Arc::new(TestChannelSubscriber::new(received_main.clone()));
        let _subscription_main = store.add_subscriber(subscriber_main).unwrap();

        // subscribed subscriber (별도 스레드에서 실행)
        let received_channeled = Arc::new(Mutex::new(Vec::new()));
        let subscriber_channeled = Box::new(TestChannelSubscriber::new(received_channeled.clone()));
        let subscription_channeled = store.subscribed(subscriber_channeled).unwrap();

        // 액션들 dispatch
        for i in 1..=5 {
            store.dispatch(i).unwrap();
        }

        thread::sleep(Duration::from_millis(100));

        store.stop().unwrap();
        subscription_channeled.unsubscribe();

        // 두 subscriber 모두 동일한 메시지를 받았는지 확인
        let states_main = received_main.lock().unwrap();
        let states_channeled = received_channeled.lock().unwrap();

        assert_eq!(states_main.len(), 5);
        assert_eq!(states_channeled.len(), 5);

        for i in 0..5 {
            assert_eq!(states_main[i], states_channeled[i]);
        }
    }
}
