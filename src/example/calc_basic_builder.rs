use std::sync::{Arc, Mutex};
use std::thread;

use rs_store::{DispatchOp, Dispatcher, StoreBuilder};
use rs_store::{Reducer, Subscriber};

#[derive(Debug)]
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
    last: Mutex<CalcState>,
}

impl Default for CalcSubscriber {
    fn default() -> CalcSubscriber {
        CalcSubscriber {
            last: Default::default(),
        }
    }
}

impl Subscriber<CalcState, CalcAction> for CalcSubscriber {
    fn on_notify(&self, state: &CalcState, action: &CalcAction) {
        if self.last.lock().unwrap().count != state.count {
            match action {
                CalcAction::Add(i) => {
                    println!(
                        "CalcSubscriber::on_notify: state:{:?} <- last {:?} + action:{}",
                        state, self.last, i
                    );
                }
                CalcAction::Subtract(i) => {
                    println!(
                        "CalcSubscriber::on_notify: state:{:?} <- last {:?} - action:{}",
                        state, self.last, i
                    );
                }
            }
        }
        //
        *self.last.lock().unwrap() = state.clone();
    }
}

pub fn main() {
    println!("Hello, Builder!");

    let store = StoreBuilder::new()
        .with_reducer(Box::new(CalcReducer::default()))
        .with_name("calc".to_string())
        .build()
        .unwrap();
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Subtract(1));

    // stop the store
    store.stop();
}
