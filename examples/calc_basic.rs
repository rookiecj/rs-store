use std::sync::{Arc, Mutex};
use std::thread;
use rs_store::{DispatchOp, Dispatcher};
use rs_store::{Reducer, Store, Subscriber};

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
    fn on_notify(&self, state: &CalcState, action: &CalcAction, epoch: u64) {
        match action {
            CalcAction::Add(_i) => {
                println!(
                    "CalcSubscriber::on_notify: state:{:?} <- last {:?} + action:{:?}, epoch: {}",
                    state, self.last.lock().unwrap(), action, epoch
                );
            }
            CalcAction::Subtract(_i) => {
                println!(
                    "CalcSubscriber::on_notify: state:{:?} <- last {:?} + action:{:?}, epoch: {}",
                    state, self.last.lock().unwrap(), action, epoch
                );
            }
        }
        //
        *self.last.lock().unwrap() = state.clone();
    }
}

pub fn main() {
    println!("Hello, Basic!");

    let store = Store::<CalcState, CalcAction>::new(Box::new(CalcReducer::default()));

    println!("add subscriber");
    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    // println!("add more subscriber");
    // store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Subtract(1));

    // stop the store
    store.stop();
}
