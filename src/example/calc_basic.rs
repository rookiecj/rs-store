use std::sync::{Arc, Mutex};
use std::thread;

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

struct CalcSubscriber {
    last: CalcState,
}

impl Default for CalcSubscriber {
    fn default() -> CalcSubscriber {
        CalcSubscriber {
            last: CalcState::default(),
        }
    }
}

impl Subscriber<CalcState, CalcAction> for CalcSubscriber {
    fn notify(&mut self, state: &CalcState, action: &CalcAction) {
        if self.last.count != state.count {
            match action {
                CalcAction::Add(i) => {
                    println!("CalcSubscriber::notify: state:{:?} <- last {:?} + action:{}", state, self.last, i);
                }
                CalcAction::Subtract(i) => {
                    println!("CalcSubscriber::notify: state:{:?} <- last {:?} - action:{}", state, self.last, i);
                }
            }
        }
        //
        self.last = state.clone();
    }
}

pub fn main() {
    println!("Hello, Calc!");

    let mut store = Store::<CalcState, CalcAction>::new();
    store.lock().unwrap().add_reducer(Box::new(CalcReducer::default()));

    store
        .lock()
        .unwrap()
        .add_subscriber(Box::new(CalcSubscriber::default()));
    store.lock().unwrap().dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store
        .lock()
        .unwrap()
        .add_subscriber(Box::new(CalcSubscriber::default()));
    store.lock().unwrap().dispatch(CalcAction::Subtract(1));

    // it should be called to close the store\
    store.lock().unwrap().close();

    // if you want to wait for the store to close
    Store::wait_for(store.clone()).unwrap_or_else(|e| {
        println!("Error: {:?}", e);
    });
}
