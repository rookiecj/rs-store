use std::sync::Arc;
use std::thread;

use rs_store::Dispatcher;
use rs_store::{Reducer, Store, Subscriber};

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
    fn reduce(&self, state: &CalcState, action: &CalcAction) -> CalcState {
        match action {
            CalcAction::Add(i) => {
                println!("CalcReducer::reduce: + {}", i);
                CalcState {
                    count: state.count + i,
                }
            }
            CalcAction::Subtract(i) => {
                println!("CalcReducer::reduce: - {}", i);
                CalcState {
                    count: state.count - i,
                }
            }
        }
    }
}

struct CalcSubscriber {}

impl Default for CalcSubscriber {
    fn default() -> CalcSubscriber {
        CalcSubscriber {}
    }
}

impl Subscriber<CalcState, CalcAction> for CalcSubscriber {
    fn notify(&mut self, state: &CalcState, action: &CalcAction) {
        match action {
            CalcAction::Add(i) => {
                println!("CalcSubscriber::notify: state:{:?}, action:{}", state, i);
            }
            CalcAction::Subtract(i) => {
                println!("CalcSubscriber::notify: state:{:?}, action: -{}", state, i);
            }
        }
    }
}

pub fn main() {
    println!("Hello, Calc!");

    let mut store = Store::<CalcState, CalcAction>::default();
    store.reducers.lock().unwrap().push(Box::new(CalcReducer::default()));
    let (tx, rx) = std::sync::mpsc::channel::<CalcAction>();
    store.tx.lock().unwrap().replace(tx);
    let dispatch_store = Arc::new(store);
    let tx_store = dispatch_store.clone();
    let dispatcher = thread::spawn(move || {
        let mut state = CalcState::default();
        for action in rx {
            dispatch_store.do_reduce(&mut state, &action);
            dispatch_store.do_notify(&state, &action);
        }
    });

    tx_store.add_subscriber(Box::new(CalcSubscriber::default()));
    tx_store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    tx_store.add_subscriber(Box::new(CalcSubscriber::default()));
    tx_store.dispatch(CalcAction::Subtract(1));
    tx_store.close();

    dispatcher.join().unwrap();
}
