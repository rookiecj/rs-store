use rs_store::DispatchOp;
use rs_store::{Dispatcher, Reducer, StoreImpl};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
enum CalcAction {
    Add(i32),
    Subtract(i32),
    Multiply(i32),
    Reset,
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
    history: Vec<String>,
}

impl Default for CalcState {
    fn default() -> CalcState {
        CalcState {
            count: 0,
            history: vec!["Initial state: 0".to_string()],
        }
    }
}

impl Reducer<CalcState, CalcAction> for CalcReducer {
    fn reduce(&self, state: &CalcState, action: &CalcAction) -> DispatchOp<CalcState, CalcAction> {
        match action {
            CalcAction::Add(n) => {
                let new_count = state.count + n;
                let mut new_history = state.history.clone();
                new_history.push(format!("Added {}: {} -> {}", n, state.count, new_count));
                DispatchOp::Dispatch(
                    CalcState {
                        count: new_count,
                        history: new_history,
                    },
                    vec![],
                )
            }
            CalcAction::Subtract(n) => {
                let new_count = state.count - n;
                let mut new_history = state.history.clone();
                new_history.push(format!(
                    "Subtracted {}: {} -> {}",
                    n, state.count, new_count
                ));
                DispatchOp::Dispatch(
                    CalcState {
                        count: new_count,
                        history: new_history,
                    },
                    vec![],
                )
            }
            CalcAction::Multiply(n) => {
                let new_count = state.count * n;
                let mut new_history = state.history.clone();
                new_history.push(format!(
                    "Multiplied by {}: {} -> {}",
                    n, state.count, new_count
                ));
                DispatchOp::Dispatch(
                    CalcState {
                        count: new_count,
                        history: new_history,
                    },
                    vec![],
                )
            }
            CalcAction::Reset => {
                let mut new_history = state.history.clone();
                new_history.push(format!("Reset: {} -> 0", state.count));
                DispatchOp::Dispatch(
                    CalcState {
                        count: 0,
                        history: new_history,
                    },
                    vec![],
                )
            }
        }
    }
}

fn main() {
    println!("=== Query State Example ===");

    // Create a store with initial state
    let store_impl =
        StoreImpl::new_with_reducer(CalcState::default(), Box::new(CalcReducer::default()))
            .unwrap();

    // Dispatch some actions
    println!("Dispatching actions...");
    store_impl.dispatch(CalcAction::Add(10)).unwrap();
    store_impl.dispatch(CalcAction::Multiply(2)).unwrap();
    store_impl.dispatch(CalcAction::Subtract(5)).unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Query the current state using query_state
    println!("\n=== Querying Current State ===");

    // Query 1: Get current count
    let current_count = Arc::new(Mutex::new(0));
    let count_clone = current_count.clone();
    store_impl
        .query_state(move |state| {
            *count_clone.lock().unwrap() = state.count;
        })
        .unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("Current count: {}", *current_count.lock().unwrap());

    // Query 2: Get history length
    let history_length = Arc::new(Mutex::new(0));
    let history_clone = history_length.clone();
    store_impl
        .query_state(move |state| {
            *history_clone.lock().unwrap() = state.history.len();
        })
        .unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("History length: {}", *history_length.lock().unwrap());

    // Query 3: Print all history
    println!("\n=== History ===");
    store_impl
        .query_state(|state| {
            for (i, entry) in state.history.iter().enumerate() {
                println!("  {}: {}", i + 1, entry);
            }
        })
        .unwrap();

    // Query 4: Check if count is even
    let is_even = Arc::new(Mutex::new(false));
    let even_clone = is_even.clone();
    store_impl
        .query_state(move |state| {
            *even_clone.lock().unwrap() = state.count % 2 == 0;
        })
        .unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("\nIs count even? {}", *is_even.lock().unwrap());

    // Query 5: Get the last operation
    let last_operation = Arc::new(Mutex::new(String::new()));
    let last_op_clone = last_operation.clone();
    store_impl
        .query_state(move |state| {
            if let Some(last) = state.history.last() {
                *last_op_clone.lock().unwrap() = last.clone();
            }
        })
        .unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("Last operation: {}", *last_operation.lock().unwrap());

    // Dispatch more actions and query again
    println!("\n=== After More Actions ===");
    store_impl.dispatch(CalcAction::Add(100)).unwrap();
    store_impl.dispatch(CalcAction::Reset).unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Query final state
    let final_count = Arc::new(Mutex::new(0));
    let final_clone = final_count.clone();
    store_impl
        .query_state(move |state| {
            *final_clone.lock().unwrap() = state.count;
        })
        .unwrap();

    // Wait for actions to be processed
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("Final count: {}", *final_count.lock().unwrap());

    // Query final history
    println!("\n=== Final History ===");
    store_impl
        .query_state(|state| {
            for (i, entry) in state.history.iter().enumerate() {
                println!("  {}: {}", i + 1, entry);
            }
        })
        .unwrap();

    // Stop the store
    store_impl.stop().unwrap();
    println!("\nStore stopped.");
}
