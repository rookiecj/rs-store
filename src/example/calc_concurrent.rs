use std::sync::{Arc, Mutex};
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
    fn notify(&self, state: &CalcState, action: &CalcAction) {
        println!("CalcSubscriber::notify: {}, action: {:?}", self.id, action);

        match action {
            CalcAction::Add(i) => {
                println!(
                    "CalcSubscriber::notify: {}, state: {:?} <- old: {:?}",
                    self.id, state, self.last
                );
            }
            CalcAction::Subtract(i) => {
                println!(
                    "CalcSubscriber::notify: {}, state: {:?} <- old: {:?}",
                    self.id, state, self.last
                );
            }
        }

        *self.last.lock().unwrap() = state.clone();
    }
}

pub fn main() {
    println!("Hello, Calc!");

    let store = Store::<CalcState, CalcAction>::new(Box::new(CalcReducer::default()));

    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Add(1));

    let store_clone = store.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_secs(1));

        store_clone.add_subscriber(Arc::new(CalcSubscriber::new(1)));
        store_clone.dispatch(CalcAction::Subtract(1));
    })
    .join()
    .unwrap();

    //store.stop();
}
