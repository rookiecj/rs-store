use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use rs_store::{DispatchOp, Dispatcher, Effect, EffectResult};
use rs_store::{Reducer, Store, Subscriber};

#[derive(Debug, Clone)]
enum CalcAction {
    Add(i32, Arc<Condvar>),
    Subtract(i32, Arc<Condvar>),
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
            CalcAction::Add(i, cond) => {
                println!("CalcReducer::reduce: + {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count + i,
                    },
                    Some(Effect::Thunk(subtract_effect_thunk(*i, cond.clone()))),
                )
            }
            CalcAction::Subtract(i, cond) => {
                println!("CalcReducer::reduce: - {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count - i,
                    },
                    Some(Effect::Function(
                        "subtract".to_string(),
                        subtract_effect_fn(*i, cond.clone()),
                    )),
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

impl Subscriber<CalcState, CalcAction> for CalcSubscriber {
    fn on_notify(&self, state: &CalcState, action: &CalcAction, epoch: u64) {
        match action {
            CalcAction::Add(_i, _) => {
                println!(
                    "CalcSubscriber::on_notify: id:{}, state: {:?} <- last: {:?} + action: {:?}, epoch: {}",
                    self.id,
                    state,
                    self.last.lock().unwrap(),
                    action,
                    epoch
                );
            }
            CalcAction::Subtract(_i, _) => {
                println!(
                    "CalcSubscriber::on_notify: id:{}, state: {:?} <- last: {:?} + action: {:?}, epoch: {}",
                    self.id,
                    state,
                    self.last.lock().unwrap(),
                    action,
                    epoch
                );
            }
        }

        *self.last.lock().unwrap() = state.clone();
    }
}

fn subtract_effect_thunk(
    i: i32,
    cond: Arc<Condvar>,
) -> Box<dyn FnOnce(Box<dyn Dispatcher<CalcAction>>) + Send> {
    Box::new(move |dispatcher| {
        println!("effect: working on long running task....");
        thread::sleep(std::time::Duration::from_secs(1));

        println!("effect: dispatching action...");
        dispatcher.dispatch(CalcAction::Subtract(i, cond.clone()))
    })
}

fn subtract_effect_fn(_i: i32, cond: Arc<Condvar>) -> Box<dyn FnOnce() -> EffectResult + Send> {
    Box::new(move || {
        println!("effect: set done");
        // set done signal
        cond.notify_all();
        Ok(Box::new(CalcState { count: 0 }))
    })
}

pub fn main() {
    println!("Hello, Effect!");

    let store = Store::<CalcState, CalcAction>::new_with_name(
        Box::new(CalcReducer::default()),
        CalcState::default(),
        "store-thunk".into(),
    )
    .unwrap();

    let lock_done = Arc::new(Mutex::new(false));
    let cond_done: Arc<Condvar> = Arc::new(Condvar::new());

    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    store.dispatch(CalcAction::Add(1, cond_done.clone()));

    drop(cond_done.wait(lock_done.lock().unwrap()).unwrap());
    thread::sleep(std::time::Duration::from_secs(1));

    store.stop();
}
