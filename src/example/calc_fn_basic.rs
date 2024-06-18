use std::sync::Arc;
use std::thread;

use rs_store::{DispatchOp, Dispatcher};
use rs_store::{FnReducer, FnSubscriber, Store};

#[derive(Debug)]
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

fn calc_reducer(state: &CalcState, action: &CalcAction) -> DispatchOp<CalcState> {
    match action {
        CalcAction::Add(i) => {
            println!("CalcReducer::reduce: + {}", i);
            DispatchOp::Dispatch(CalcState {
                count: state.count + i,
            })
        }
        CalcAction::Subtract(i) => {
            println!("CalcReducer::reduce: - {}", i);
            DispatchOp::Dispatch(CalcState {
                count: state.count - i,
            })
        }
    }
}

fn calc_subscriber(state: &CalcState, action: &CalcAction) {
    match action {
        CalcAction::Add(i) => {
            println!("CalcSubscriber::on_notify: state:{:?}, action:{}", state, i);
        }
        CalcAction::Subtract(i) => {
            println!(
                "CalcSubscriber::on_notify: state:{:?}, action: -{}",
                state, i
            );
        }
    }
}

pub fn main() {
    println!("Hello, reduce function !");

    let store = Store::<CalcState, CalcAction>::new(Box::new(FnReducer::from(calc_reducer)));

    store.add_subscriber(Arc::new(FnSubscriber::from(calc_subscriber)));
    store.dispatch(CalcAction::Add(1));

    thread::sleep(std::time::Duration::from_secs(1));
    store.add_subscriber(Arc::new(FnSubscriber::from(calc_subscriber)));
    store.dispatch(CalcAction::Subtract(1));

    store.stop();
}
