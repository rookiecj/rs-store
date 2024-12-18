use crate::Effect;
use crate::StoreError;

pub trait Middleware<State, Action>: Send + Sync
where
    State: Send + Sync,
    Action: Send + Sync,
{
    fn before_dispatch(&mut self, action: &Action, state: &State) -> Result<(), StoreError>;
    fn after_dispatch(
        &mut self,
        action: &Action,
        old_state: &State,
        new_state: &State,
        effects: &Vec<Effect<Action>>,
    ) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::{DispatchOp, Reducer, StoreBuilder};

    use super::*;

    struct TestReducer;
    //     Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync + 'static,
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            let new_state = state + action;
            DispatchOp::Dispatch(new_state, None)
        }
    }

    struct LoggerMiddleware {
        logs: Vec<String>,
    }

    impl LoggerMiddleware {
        fn new() -> Self {
            Self { logs: vec![] }
        }
    }

    impl<State, Action> Middleware<State, Action> for LoggerMiddleware
    where
        State: std::fmt::Debug + Send + Sync,
        Action: std::fmt::Debug + Send + Sync,
    {
        fn before_dispatch(&mut self, action: &Action, state: &State) -> Result<(), StoreError> {
            let log = format!("Before dispatch - Action: {:?}, state: {:?}", action, state);
            println!("{}", log);
            self.logs.push(log);
            Ok(())
        }

        fn after_dispatch(
            &mut self,
            action: &Action,
            old_state: &State,
            new_state: &State,
            _effects: &Vec<Effect<Action>>,
        ) -> Result<(), StoreError> {
            let log = format!(
                "After dispatch - Action: {:?}, state: {:?} -> {:?}",
                action, old_state, new_state
            );
            println!("{}", log);
            self.logs.push(log);
            Ok(())
        }
    }

    #[test]
    fn test_middleware() {
        let reducer = Box::new(TestReducer);
        let store_result = StoreBuilder::new().with_reducer(reducer).build();

        assert!(store_result.is_ok());

        let store = store_result.unwrap();
        // Add logger middleware
        let logger = Arc::new(Mutex::new(LoggerMiddleware::new()));
        store.add_middleware(logger.clone());

        let _ = store.dispatch(1);
        store.stop();

        assert_eq!(logger.lock().unwrap().logs.len(), 2);
        assert!(logger.lock().unwrap().logs.first().unwrap().contains("Before dispatch"));
        assert!(logger.lock().unwrap().logs.last().unwrap().contains("After dispatch"));
    }
}
