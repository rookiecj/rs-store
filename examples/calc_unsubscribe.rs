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
    fn reduce(&self, state: &CalcState, action: &CalcAction) -> DispatchOp<CalcState, CalcAction> {
        match action {
            CalcAction::Add(i) => {
                println!("CalcReducer::reduce: + {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count + i,
                    },
                    None,
                )
            }
            CalcAction::Subtract(i) => {
                println!("CalcReducer::reduce: - {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count - i,
                    },
                    None,
                )
            }
        }
    }
}

struct CalcSubscriber {
    id: i32,
    last: Mutex<CalcState>,
}

impl Default for CalcSubscriber {
    fn default() -> CalcSubscriber {
        CalcSubscriber {
            id: 0,
            last: Mutex::new(CalcState::default()),
        }
    }
}

impl CalcSubscriber {
    fn new(id: i32) -> CalcSubscriber {
        CalcSubscriber {
            id,
            last: Mutex::new(CalcState::default()),
        }
    }
}

impl Subscriber<CalcState, CalcAction> for CalcSubscriber {
    fn on_notify(&self, state: &CalcState, action: &CalcAction) {
        match action {
            CalcAction::Add(_i) => {
                println!(
                    "CalcSubscriber::on_notify: id:{}, state: {:?} <- last: {:?} + action: {:?}",
                    self.id,
                    state,
                    self.last.lock().unwrap(),
                    action,
                );
            }
            CalcAction::Subtract(_i) => {
                println!(
                    "CalcSubscriber::on_notify: id:{}, state: {:?} <- last: {:?} + action: {:?}",
                    self.id,
                    state,
                    self.last.lock().unwrap(),
                    action,
                );
            }
        }

        *self.last.lock().unwrap() = state.clone();
    }
}

pub fn main() {
    println!("Hello, Unsubscribe!");

    let store = StoreBuilder::new_with_reducer(Box::new(CalcReducer::default()))
        .with_state(CalcState::default())
        .with_name("store-unsubscribe".into())
        .build()
        .unwrap();

    println!("add subscriber");
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    let _ = store.dispatch(CalcAction::Add(1));

    let store_clone = store.clone();
    let handle = thread::spawn(move || {
        thread::sleep(std::time::Duration::from_secs(1));

        // subscribe
        println!("add more subscriber");
        let subscription = store_clone.add_subscriber(Arc::new(CalcSubscriber::new(1)));
        let _ = store_clone.dispatch(CalcAction::Subtract(1));
        subscription
    });

    let subscription = handle.join().unwrap();

    println!("Unsubscribing...");
    subscription.unsubscribe();

    println!("Send 42...");
    let _ = store.dispatch(CalcAction::Add(42));

    store.stop();

    println!("Done!");
}
