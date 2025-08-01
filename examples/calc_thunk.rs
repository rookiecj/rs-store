use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use rs_store::{DispatchOp, Dispatcher, StoreBuilder};
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
    fn default() -> Self {
        Self {
            id: 0,
            last: Mutex::new(CalcState::default()),
        }
    }
}

// impl CalcSubscriber {
//     fn new(id: i32) -> Self {
//         Self {
//             id,
//             last: Mutex::new(CalcState::default()),
//         }
//     }
// }

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

// now we can have a thunk before create the store
fn get_subtract_thunk(
    cond: Arc<Condvar>,
    i: i32,
) -> Box<dyn FnOnce(Box<dyn Dispatcher<CalcAction>>) + Send> {
    Box::new(move |dispatcher| {
        println!("thunk: working on long running task....");
        thread::sleep(std::time::Duration::from_secs(1));

        println!("thunk: dispatching action...");
        // set done signal
        cond.notify_all();
        dispatcher.dispatch(CalcAction::Subtract(i)).expect("no dispatch failed");
    })
}

pub fn main() {
    println!("Hello, Thunk!");

    // create a thunk somewhere else
    let lock_done = Arc::new(Mutex::new(false));
    let cond_done: Arc<Condvar> = Arc::new(Condvar::new());
    let subtract_thunk = get_subtract_thunk(cond_done.clone(), 1);

    let store =
        StoreBuilder::new_with_reducer(CalcState::default(), Box::new(CalcReducer::default()))
            .with_name("store-thunk".into())
            .build()
            .unwrap();

    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Add(1)).expect("no dispatch failed");

    // send thunk to store
    store.dispatch_thunk(subtract_thunk);

    // wait for thunk to finish
    drop(cond_done.wait(lock_done.lock().unwrap()).unwrap());

    match store.stop() {
        Ok(_) => println!("store stopped"),
        Err(e) => {
            panic!("store stop failed  : {:?}", e);
        }
    }
}
