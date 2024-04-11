use std::sync::{Arc, Mutex};
use std::thread;
use store_rs::{Store, Reducer, Subscriber};

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
    fn notify(&self, state: &CalcState, action: &CalcAction) {
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
    store.reducers.push(Box::new(CalcReducer::default()));
    let (tx, rx) = std::sync::mpsc::channel::<CalcAction>();
    store.tx = Some(tx);
    let dispatch_store = Arc::new(Mutex::new(store));
    let tx_store = dispatch_store.clone();
    let dispatcher = thread::spawn(move || {
        for action in rx {
            let new_state = dispatch_store.lock().unwrap().do_reduce(&action);
            dispatch_store.lock().unwrap().do_notify(&new_state, &action);
        }
    });

    tx_store.lock().unwrap().add_subscriber(Box::new(CalcSubscriber::default()));
    tx_store.lock().unwrap().dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    tx_store.lock().unwrap().add_subscriber(Box::new(CalcSubscriber::default()));
    tx_store.lock().unwrap().dispatch(CalcAction::Subtract(1));
    tx_store.lock().unwrap().close();

    dispatcher.join().unwrap();
}