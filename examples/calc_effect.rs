use std::sync::{Arc, Mutex};
use std::thread;

use rs_store::{DispatchOp, Dispatcher, Effect, EffectResult, StoreBuilder};
use rs_store::{Reducer, Subscriber};

#[derive(Debug, Clone)]
enum CalcAction {
    AddWillProduceThunk(i32),
    SubtractWillProduceEffectFunction(i32),
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
            CalcAction::AddWillProduceThunk(i) => {
                println!("CalcReducer::reduce: + {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count + i,
                    },
                    Some(Effect::Thunk(subtract_effect_thunk(*i))),
                )
            }
            CalcAction::SubtractWillProduceEffectFunction(i) => {
                println!("CalcReducer::reduce: - {}", i);
                DispatchOp::Dispatch(
                    CalcState {
                        count: state.count - i,
                    },
                    // produce effect function
                    Some(Effect::Function(
                        "subtract".to_string(),
                        subtract_effect_fn(*i),
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
    fn on_notify(&self, state: &CalcState, action: &CalcAction) {
        match action {
            CalcAction::AddWillProduceThunk(_i) => {
                println!(
                    "CalcSubscriber::on_notify: id:{}, state: {:?} <- last: {:?} + action: {:?}",
                    self.id,
                    state,
                    self.last.lock().unwrap(),
                    action,
                );
            }
            CalcAction::SubtractWillProduceEffectFunction(_i) => {
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

fn subtract_effect_thunk(
    i: i32,
) -> Box<dyn FnOnce(Box<dyn Dispatcher<CalcAction>>) + Send> {
    Box::new(move |dispatcher| {
        println!("effect: working on long running task....");
        thread::sleep(std::time::Duration::from_secs(1));

        println!("effect: dispatching action...");
        dispatcher.dispatch(CalcAction::SubtractWillProduceEffectFunction(i))
    })
}

fn subtract_effect_fn(_i: i32) -> Box<dyn FnOnce() -> EffectResult + Send> {
    Box::new(move || {
        println!("effect: working on long running task again....");
        thread::sleep(std::time::Duration::from_secs(1));

        println!("effect: now returns result");
        Ok(Box::new(CalcState { count: 0 }))
    })
}

pub fn main() {
    println!("Hello, Effect!");

    let store = StoreBuilder::new_with_reducer(Box::new(CalcReducer::default()))
        .with_state(CalcState::default())
        .with_name("store-effect".into())
        .build()
        .unwrap();

    store.add_subscriber(Arc::new(CalcSubscriber::default()));
    let _ = store.dispatch(CalcAction::AddWillProduceThunk(1));

    store.stop();
}
