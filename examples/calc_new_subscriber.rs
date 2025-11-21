use std::sync::{Arc, Mutex};
use std::thread;

use rs_store::{DispatchOp, StoreBuilder};
use rs_store::{Reducer, Subscriber};

#[derive(Debug, Clone)]
enum CalcAction {
    Add(i32),
    Subtract(i32),
}

struct CalcReducer {}

impl Default for CalcReducer {
    fn default() -> CalcReducer {
        CalcReducer {}
    }
}

#[derive(Debug, Clone)]
struct CalcState {
    count: i32,
}

impl Default for CalcState {
    fn default() -> CalcState {
        CalcState { count: 0 }
    }
}

impl Reducer<CalcState, CalcAction> for CalcReducer {
    fn reduce(&self, state: CalcState, action: CalcAction) -> DispatchOp<CalcState, CalcAction> {
        match action {
            CalcAction::Add(i) => {
                println!("CalcReducer::reduce: + {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count + i,
                    },
                    vec![],
                )
            }
            CalcAction::Subtract(i) => {
                println!("CalcReducer::reduce: - {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count - i,
                    },
                    vec![],
                )
            }
        }
    }
}

// 새로운 subscriber가 추가될 때 최신 상태를 받는 subscriber
struct CalcSubscriberWithSubscribe {
    id: i32,
    last: Mutex<CalcState>,
    subscribe_called: Mutex<bool>,
    subscribe_state: Mutex<Option<CalcState>>,
}

impl CalcSubscriberWithSubscribe {
    fn new(id: i32) -> Self {
        Self {
            id,
            last: Mutex::new(CalcState::default()),
            subscribe_called: Mutex::new(false),
            subscribe_state: Mutex::new(None),
        }
    }

    fn was_subscribe_called(&self) -> bool {
        *self.subscribe_called.lock().unwrap()
    }

    fn get_subscribe_state(&self) -> Option<CalcState> {
        self.subscribe_state.lock().unwrap().clone()
    }
}

impl Subscriber<CalcState, CalcAction> for CalcSubscriberWithSubscribe {
    fn on_subscribe(&self, state: CalcState) {
        println!(
            "CalcSubscriber::on_subscribe: id:{}, received initial state: {:?}",
            self.id, state
        );
        *self.subscribe_called.lock().unwrap() = true;
        *self.subscribe_state.lock().unwrap() = Some(state.clone());
        *self.last.lock().unwrap() = state.clone();
    }

    fn on_notify(&self, state: CalcState, action: CalcAction) {
        println!(
            "CalcSubscriber::on_notify: id:{}, state: {:?} <- last: {:?} + action: {:?}",
            self.id,
            state,
            self.last.lock().unwrap(),
            action,
        );

        *self.last.lock().unwrap() = state.clone();
    }
}

pub fn main() {
    println!("Hello, New Subscriber with Latest State!");

    let store =
        StoreBuilder::new_with_reducer(CalcState::default(), Box::new(CalcReducer::default()))
            .with_name("calc-new-subscriber".into())
            .build()
            .unwrap();

    // 첫 번째 subscriber 추가
    println!("=== Adding first subscriber ===");
    let subscriber1 = Arc::new(CalcSubscriberWithSubscribe::new(1));
    store.add_subscriber(subscriber1.clone()).unwrap();

    // 액션을 dispatch하여 상태 변경
    println!("=== Dispatching actions ===");
    store.dispatch(CalcAction::Add(5)).unwrap();
    store.dispatch(CalcAction::Add(10)).unwrap();

    // 잠시 대기하여 액션이 처리되도록 함
    thread::sleep(std::time::Duration::from_millis(100));

    // 두 번째 subscriber 추가 (현재 상태는 count: 15)
    println!("=== Adding second subscriber ===");
    let subscriber2 = Arc::new(CalcSubscriberWithSubscribe::new(2));
    store.add_subscriber(subscriber2.clone()).unwrap();

    // 잠시 대기하여 AddSubscriber 액션이 처리되도록 함
    thread::sleep(std::time::Duration::from_millis(100));

    // 새로운 액션을 dispatch
    println!("=== Dispatching more actions ===");
    store.dispatch(CalcAction::Subtract(3)).unwrap();
    store.dispatch(CalcAction::Add(7)).unwrap();

    // 잠시 대기하여 액션이 처리되도록 함
    thread::sleep(std::time::Duration::from_millis(100));

    // 결과 확인
    println!("=== Results ===");
    println!("Final state: {:?}", store.get_state());

    // 첫 번째 subscriber 확인
    println!("Subscriber 1:");
    println!("  Subscribe called: {}", subscriber1.was_subscribe_called());
    if let Some(state) = subscriber1.get_subscribe_state() {
        println!("  Initial state received: {:?}", state);
    }
    println!("  Last state: {:?}", *subscriber1.last.lock().unwrap());

    // 두 번째 subscriber 확인
    println!("Subscriber 2:");
    println!("  Subscribe called: {}", subscriber2.was_subscribe_called());
    if let Some(state) = subscriber2.get_subscribe_state() {
        println!("  Initial state received: {:?}", state);
    }
    println!("  Last state: {:?}", *subscriber2.last.lock().unwrap());

    // stop the store
    match store.stop() {
        Ok(_) => println!("store stopped"),
        Err(e) => {
            panic!("store stop failed  : {:?}", e);
        }
    }

    println!("Done!");
}
