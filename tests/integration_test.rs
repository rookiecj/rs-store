use rs_store::{DispatchOp, Reducer, StoreBuilder, Subscriber};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
enum TestAction {
    Increment,
    Decrement,
    SetValue(i32),
}

#[derive(Debug, Clone, PartialEq)]
struct TestState {
    counter: i32,
    name: String,
}

impl Default for TestState {
    fn default() -> Self {
        TestState {
            counter: 0,
            name: "default".to_string(),
        }
    }
}

struct TestReducer;

impl Reducer<TestState, TestAction> for TestReducer {
    fn reduce(&self, state: &TestState, action: &TestAction) -> DispatchOp<TestState, TestAction> {
        match action {
            TestAction::Increment => {
                let new_state = TestState {
                    counter: state.counter + 1,
                    name: state.name.clone(),
                };
                DispatchOp::Dispatch(new_state, vec![])
            }
            TestAction::Decrement => {
                let new_state = TestState {
                    counter: state.counter - 1,
                    name: state.name.clone(),
                };
                DispatchOp::Dispatch(new_state, vec![])
            }
            TestAction::SetValue(value) => {
                let new_state = TestState {
                    counter: *value,
                    name: state.name.clone(),
                };
                DispatchOp::Dispatch(new_state, vec![])
            }
        }
    }
}

#[allow(dead_code)]
struct IntegrationTestSubscriber {
    id: String,
    received_states: Arc<Mutex<Vec<TestState>>>,
    received_actions: Arc<Mutex<Vec<TestAction>>>,
    subscribe_called: Arc<Mutex<bool>>,
    initial_state: Arc<Mutex<Option<TestState>>>,
}

#[allow(dead_code)]
impl IntegrationTestSubscriber {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            received_states: Arc::new(Mutex::new(Vec::new())),
            received_actions: Arc::new(Mutex::new(Vec::new())),
            subscribe_called: Arc::new(Mutex::new(false)),
            initial_state: Arc::new(Mutex::new(None)),
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

    fn get_initial_state(&self) -> Option<TestState> {
        self.initial_state.lock().unwrap().clone()
    }
}

impl Subscriber<TestState, TestAction> for IntegrationTestSubscriber {
    fn on_subscribe(&self, state: &TestState) {
        // println!("[{}] on_subscribe called with state: {:?}", self.id, state);
        *self.subscribe_called.lock().unwrap() = true;
        *self.initial_state.lock().unwrap() = Some(state.clone());
        self.received_states.lock().unwrap().push(state.clone());
    }

    fn on_notify(&self, state: &TestState, action: &TestAction) {
        // println!(
        //     "[{}] on_notify called with state: {:?}, action: {:?}",
        //     self.id, state, action
        // );
        self.received_states.lock().unwrap().push(state.clone());
        self.received_actions.lock().unwrap().push(action.clone());
    }
}

#[test]
fn test_integration_new_subscriber_feature() {
    println!();
    println!("=== Integration Test: New Subscriber Feature ===");

    // Store 생성
    let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
        .with_name("integration-test".into())
        .build()
        .unwrap();

    // 첫 번째 subscriber 추가
    println!("Adding first subscriber...");
    let subscriber1 = Arc::new(IntegrationTestSubscriber::new("subscriber-1"));
    store.add_subscriber(subscriber1.clone()).unwrap();

    // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
    thread::sleep(Duration::from_millis(200));

    // 첫 번째 subscriber가 초기 상태를 받았는지 확인
    assert!(subscriber1.was_subscribe_called());
    assert_eq!(subscriber1.get_received_states().len(), 1);
    assert_eq!(subscriber1.get_received_states()[0].counter, 0);

    // 액션들을 dispatch하여 상태 변경
    println!("Dispatching actions...");
    store.dispatch(TestAction::Increment).unwrap();
    store.dispatch(TestAction::Increment).unwrap();
    store.dispatch(TestAction::SetValue(10)).unwrap();

    // 잠시 대기하여 액션이 처리되도록 함
    thread::sleep(Duration::from_millis(100));

    // 첫 번째 subscriber가 모든 상태 변경을 받았는지 확인
    let states1 = subscriber1.get_received_states();
    let actions1 = subscriber1.get_received_actions();
    assert_eq!(states1.len(), 4); // on_subscribe + 3 actions
    assert_eq!(actions1.len(), 3); // 3 actions
    assert_eq!(states1[1].counter, 1); // 첫 번째 Increment
    assert_eq!(states1[2].counter, 2); // 두 번째 Increment
    assert_eq!(states1[3].counter, 10); // SetValue(10)

    // 두 번째 subscriber 추가 (현재 상태는 counter: 10)
    println!("Adding second subscriber...");
    let subscriber2 = Arc::new(IntegrationTestSubscriber::new("subscriber-2"));
    store.add_subscriber(subscriber2.clone()).unwrap();

    // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
    thread::sleep(Duration::from_millis(100));

    // 두 번째 subscriber가 현재 상태를 받았는지 확인
    assert!(subscriber2.was_subscribe_called());
    assert_eq!(subscriber2.get_received_states().len(), 1);
    assert_eq!(subscriber2.get_received_states()[0].counter, 10);

    // 새로운 액션을 dispatch
    println!("Dispatching more actions...");
    store.dispatch(TestAction::Decrement).unwrap();
    store.dispatch(TestAction::Increment).unwrap();

    // 잠시 대기하여 액션이 처리되도록 함
    thread::sleep(Duration::from_millis(100));

    // 두 subscriber 모두 새로운 액션을 받았는지 확인
    let states1_final = subscriber1.get_received_states();
    let states2_final = subscriber2.get_received_states();

    assert_eq!(states1_final.len(), 6); // on_subscribe + 5 actions
    assert_eq!(states2_final.len(), 3); // on_subscribe + 2 actions

    assert_eq!(states1_final[4].counter, 9); // Decrement
    assert_eq!(states1_final[5].counter, 10); // Increment

    assert_eq!(states2_final[1].counter, 9); // Decrement
    assert_eq!(states2_final[2].counter, 10); // Increment

    println!("Integration test completed successfully!");
}

#[test]
fn test_integration_concurrent_subscribers() {
    println!();
    println!("=== Integration Test: Concurrent Subscribers ===");

    let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
        .with_name("concurrent-test".into())
        .build()
        .unwrap();

    // 여러 스레드에서 동시에 subscriber 추가
    let mut handles = vec![];
    let store_clone = store.clone();

    for i in 0..5 {
        let store_thread = store_clone.clone();
        let handle = thread::spawn(move || {
            let subscriber = Arc::new(IntegrationTestSubscriber::new(&format!("thread-{}", i)));
            store_thread.add_subscriber(subscriber.clone()).unwrap();

            // 잠시 대기
            thread::sleep(Duration::from_millis(50));

            // 액션 dispatch
            store_thread.dispatch(TestAction::Increment).unwrap();

            subscriber
        });
        handles.push(handle);
    }

    // 모든 스레드 완료 대기
    let subscribers: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // 잠시 대기하여 모든 액션이 처리되도록 함
    thread::sleep(Duration::from_millis(200));

    // 모든 subscriber가 정상적으로 작동했는지 확인
    for subscriber in &subscribers {
        assert!(subscriber.was_subscribe_called());
        let states = subscriber.get_received_states();
        let actions = subscriber.get_received_actions();

        // 최소한 on_subscribe와 하나의 액션을 받았어야 함
        assert!(states.len() >= 2);
        assert!(actions.len() >= 1);
    }

    println!("Concurrent subscribers test completed successfully!");
}

#[test]
fn test_integration_subscriber_lifecycle() {
    println!();
    println!("=== Integration Test: Subscriber Lifecycle ===");

    let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
        .with_name("lifecycle-test".into())
        .build()
        .unwrap();

    // subscriber 추가
    let subscriber = Arc::new(IntegrationTestSubscriber::new("lifecycle"));
    let subscription = store.add_subscriber(subscriber.clone()).unwrap();

    // 잠시 대기
    thread::sleep(Duration::from_millis(100));

    // 초기 상태 확인
    assert!(subscriber.was_subscribe_called());
    assert_eq!(subscriber.get_received_states().len(), 1);

    // 액션 dispatch
    store.dispatch(TestAction::Increment).unwrap();
    thread::sleep(Duration::from_millis(100));

    // 액션 수신 확인
    assert_eq!(subscriber.get_received_states().len(), 2);
    assert_eq!(subscriber.get_received_actions().len(), 1);

    // unsubscribe
    subscription.unsubscribe();

    // 추가 액션 dispatch
    store.dispatch(TestAction::Increment).unwrap();
    thread::sleep(Duration::from_millis(100));

    // unsubscribe 후에는 액션을 받지 않아야 함
    assert_eq!(subscriber.get_received_states().len(), 2);
    assert_eq!(subscriber.get_received_actions().len(), 1);

    println!("Subscriber lifecycle test completed successfully!");
}
