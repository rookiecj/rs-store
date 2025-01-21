use std::sync::Arc;
use std::thread;

use rs_store::{DispatchOp, StoreBuilder};
use rs_store::{FnReducer, FnSubscriber};

#[derive(Debug, Clone)]
enum CalcAction {
    Add(i32),
    Subtract(i32),
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

fn calc_reducer(state: &CalcState, action: &CalcAction) -> DispatchOp<CalcState, CalcAction> {
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

fn calc_subscriber(state: &CalcState, action: &CalcAction) {
    match action {
        CalcAction::Add(i) => {
            println!("CalcSubscriber::on_notify: state:{:?}, action:{}", state, i);
        }
        CalcAction::Subtract(i) => {
            println!("CalcSubscriber::on_notify: state:{:?}, action:{}", state, i);
        }
    }
}

pub fn main() {
    println!("Hello, reduce function !");

    let store = StoreBuilder::new_with_reducer(
        CalcState::default(),
        Box::new(FnReducer::from(calc_reducer)),
    )
    .with_name("store-reduce-fn".into())
    .build()
    .unwrap();

    store.add_subscriber(Arc::new(FnSubscriber::from(calc_subscriber)));
    let _ = store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store.add_subscriber(Arc::new(FnSubscriber::from(calc_subscriber)));
    let _ = store.dispatch(CalcAction::Subtract(1));

    store.stop();
}
