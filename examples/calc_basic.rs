use rs_store::DispatchOp;
use rs_store::Dispatcher;
use rs_store::{Reducer, StoreImpl, Subscriber};
use std::sync::{Arc, Mutex};

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

#[derive(Debug)]
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
        match action {
            CalcAction::Add(_i) => {
                println!(
                    "CalcSubscriber::on_notify: state:{:?} <- last {:?} + action:{:?}",
                    state,
                    self.last.lock().unwrap(),
                    action
                );
            }
            CalcAction::Subtract(_i) => {
                println!(
                    "CalcSubscriber::on_notify: state:{:?} <- last {:?} + action:{:?}",
                    state,
                    self.last.lock().unwrap(),
                    action
                );
            }
        }
        //
        *self.last.lock().unwrap() = state.clone();
    }
}

pub fn main() {
    println!("Hello, Basic!");

    let store = StoreImpl::<CalcState, CalcAction>::new_with_reducer(
        CalcState::default(),
        Box::new(CalcReducer::default()),
    )
    .unwrap();

    println!("add subscriber");
    store.add_subscriber(Arc::new(CalcSubscriber::default())).unwrap();
    store.dispatch(CalcAction::Add(1)).expect("no dispatch failed");
    store.dispatch(CalcAction::Subtract(1)).expect("no dispatch failed");

    // stop the store
    match store.stop() {
        Ok(_) => println!("store stopped"),
        Err(e) => {
            panic!("store stop failed  : {:?}", e);
        }
    }

    assert_eq!(store.get_state().count, 0);
}
