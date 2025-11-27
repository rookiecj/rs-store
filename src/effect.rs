use crate::Dispatcher;

/// Represents a side effect that can be executed.
///
/// `Effect` is used to encapsulate actions that should be performed as a result of a state change.
/// an effect can be either simple function or more complex thunk that require a dispatcher.
pub enum Effect<Action> {
    /// An action that should be dispatched.
    Action(Action),
    /// A task which will be executed asynchronously.
    Task(Box<dyn FnOnce() + Send>),
    /// A task that takes the dispatcher as an argument.
    Thunk(Box<dyn FnOnce(Box<dyn Dispatcher<Action>>) + Send>),
    /// A function which has a result.
    /// The result is an Any type which can be downcasted to the expected type,
    /// It is useful when you want to produce an effect without any dependency of 'store'
    ///
    /// ### Caution
    /// The result default ignored, if you want to get the result of the function,
    /// you can use a middleware like `TestEffectMiddleware`
    Function(String, EffectFunction),
}

pub type EffectResult = Result<Box<dyn std::any::Any>, Box<dyn std::error::Error>>;
pub type EffectFunction = Box<dyn FnOnce() -> EffectResult + Send>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DispatchOp, MiddlewareFn, MiddlewareFnFactory, Reducer, StoreBuilder, StoreError,
        Subscriber,
    };
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    // Test state and actions for Effect tests
    #[derive(Debug, Clone, PartialEq)]
    struct TestState {
        value: i32,
        messages: Vec<String>,
    }

    impl Default for TestState {
        fn default() -> Self {
            TestState {
                value: 0,
                messages: Vec::new(),
            }
        }
    }

    #[derive(Debug, Clone)]
    enum TestAction {
        SetValue(i32),
        AddValue(i32),
        AddMessage(String),
        AsyncTask,
        ThunkTask(i32),
        FunctionTask,
    }

    // Test reducer that produces different types of effects
    struct TestReducer;

    impl Reducer<TestState, TestAction> for TestReducer {
        fn reduce(
            &self,
            state: &TestState,
            action: &TestAction,
        ) -> crate::DispatchOp<TestState, TestAction> {
            match action {
                TestAction::SetValue(value) => {
                    let new_state = TestState {
                        value: *value,
                        messages: state.messages.clone(),
                    };
                    // Effect::Action - dispatch another action
                    let effect =
                        Effect::Action(TestAction::AddMessage(format!("Set to {}", value)));
                    crate::DispatchOp::Dispatch(new_state, vec![effect])
                }
                TestAction::AddValue(value) => {
                    let new_state = TestState {
                        value: state.value + value,
                        messages: state.messages.clone(),
                    };
                    crate::DispatchOp::Dispatch(new_state, vec![])
                }
                TestAction::AddMessage(msg) => {
                    let mut new_messages = state.messages.clone();
                    new_messages.push(msg.clone());
                    let new_state = TestState {
                        value: state.value,
                        messages: new_messages,
                    };
                    crate::DispatchOp::Dispatch(new_state, vec![])
                }
                TestAction::AsyncTask => {
                    let new_state = TestState {
                        value: state.value,
                        messages: state.messages.clone(),
                    };
                    // Effect::Task - simple async task
                    let effect = Effect::Task(Box::new(|| {
                        thread::sleep(Duration::from_millis(10));
                    }));
                    crate::DispatchOp::Dispatch(new_state, vec![effect])
                }
                TestAction::ThunkTask(value) => {
                    let new_state = TestState {
                        value: state.value,
                        messages: state.messages.clone(),
                    };
                    // Effect::Thunk - task that uses dispatcher
                    let value_clone = *value; // Clone the value to avoid lifetime issues
                    let effect = Effect::Thunk(Box::new(move |dispatcher| {
                        thread::sleep(Duration::from_millis(10));
                        let _ = dispatcher.dispatch(TestAction::AddValue(value_clone));
                    }));
                    crate::DispatchOp::Dispatch(new_state, vec![effect])
                }
                TestAction::FunctionTask => {
                    let new_state = TestState {
                        value: state.value,
                        messages: state.messages.clone(),
                    };
                    // Effect::Function - function that returns a result
                    // key == test-key
                    let key = "test-key".to_string();
                    let effect = Effect::Function(
                        key.clone(),
                        Box::new(move || {
                            thread::sleep(Duration::from_millis(10));
                            Ok(Box::new(format!("Result for {}", key)) as Box<dyn std::any::Any>)
                        }),
                    );
                    crate::DispatchOp::Dispatch(new_state, vec![effect])
                }
            }
        }
    }

    // Test subscriber to track state changes
    #[derive(Default)]
    struct TestSubscriber {
        states: Arc<Mutex<Vec<TestState>>>,
        actions: Arc<Mutex<Vec<TestAction>>>,
    }

    impl Subscriber<TestState, TestAction> for TestSubscriber {
        fn on_notify(&self, state: &TestState, action: &TestAction) {
            self.states.lock().unwrap().push(state.clone());
            self.actions.lock().unwrap().push(action.clone());
        }
    }

    impl TestSubscriber {
        fn get_states(&self) -> Vec<TestState> {
            self.states.lock().unwrap().clone()
        }

        fn get_actions(&self) -> Vec<TestAction> {
            self.actions.lock().unwrap().clone()
        }
    }

    #[test]
    fn test_effect_action() {
        // Test Effect::Action - simple action dispatch
        println!("Testing Effect::Action");

        let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
            .with_name("test-action-effect".into())
            .build()
            .unwrap();

        let subscriber = Arc::new(TestSubscriber::default());
        store.add_subscriber(subscriber.clone()).unwrap();

        // Dispatch action that produces Effect::Action
        store.dispatch(TestAction::SetValue(42)).unwrap();

        // Wait for effect to be processed
        thread::sleep(Duration::from_millis(100));

        // Stop store to ensure all effects are processed
        store.stop().unwrap();

        let states = subscriber.get_states();
        let actions = subscriber.get_actions();

        // Should have received the initial state and the SetValue action
        assert!(states.len() >= 1);
        assert!(actions.len() >= 1);

        // The last state should have value 42
        assert_eq!(states.last().unwrap().value, 42);

        // Should have received both SetValue and AddMessage actions
        assert!(actions.iter().any(|a| matches!(a, TestAction::SetValue(42))));
        assert!(actions.iter().any(|a| matches!(a, TestAction::AddMessage(_))));

        println!("Effect::Action test passed");
    }

    #[test]
    fn test_effect_task() {
        // Test Effect::Task - async task execution
        println!("Testing Effect::Task");

        let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
            .with_name("test-task-effect".into())
            .build()
            .unwrap();

        let subscriber = Arc::new(TestSubscriber::default());
        store.add_subscriber(subscriber.clone()).unwrap();

        // Dispatch action that produces Effect::Task
        store.dispatch(TestAction::AsyncTask).unwrap();

        // Wait for effect to be processed
        thread::sleep(Duration::from_millis(100));

        // Stop store to ensure all effects are processed
        store.stop().unwrap();

        let actions = subscriber.get_actions();

        // Should have received the AsyncTask action
        assert!(actions.iter().any(|a| matches!(a, TestAction::AsyncTask)));

        println!("Effect::Task test passed");
    }

    #[test]
    fn test_effect_thunk() {
        // Test Effect::Thunk - task that uses dispatcher
        println!("Testing Effect::Thunk");

        let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
            .with_name("test-thunk-effect".into())
            .build()
            .unwrap();

        let subscriber = Arc::new(TestSubscriber::default());
        store.add_subscriber(subscriber.clone()).unwrap();

        // Dispatch action that produces Effect::Thunk
        store.dispatch(TestAction::ThunkTask(10)).unwrap();

        // Wait for effect to be processed
        thread::sleep(Duration::from_millis(100));

        // Stop store to ensure all effects are processed
        store.stop().unwrap();

        let states = subscriber.get_states();
        let actions = subscriber.get_actions();

        // Should have received the ThunkTask action
        assert!(actions.iter().any(|a| matches!(a, TestAction::ThunkTask(10))));

        // The thunk should have dispatched AddValue action
        assert!(actions.iter().any(|a| matches!(a, TestAction::AddValue(10))));

        // Final state should have value 10
        assert_eq!(states.last().unwrap().value, 10);

        println!("Effect::Thunk test passed");
    }

    struct TestEffectMiddleware;
    impl TestEffectMiddleware {
        fn new() -> Self {
            Self {}
        }
    }
    impl MiddlewareFnFactory<TestState, TestAction> for TestEffectMiddleware {
        fn create(
            &self,
            inner: MiddlewareFn<TestState, TestAction>,
        ) -> MiddlewareFn<TestState, TestAction> {
            Arc::new(move |state: &TestState, action: &TestAction| {
                // inner
                let result: Result<DispatchOp<TestState, TestAction>, StoreError> =
                    inner(state, action);

                // effects 를 순회하면서 function 을 변환
                let (need_to_dispatch, state, effects) = match result {
                    Ok(DispatchOp::Dispatch(state, effects)) => (true, state, effects),
                    Ok(DispatchOp::Keep(state, effects)) => (false, state, effects),
                    Err(e) => {
                        return Err(e);
                    }
                };

                // convert function effect to task effect if key is "test-key"
                let new_effects: Vec<Effect<TestAction>> = effects
                    .into_iter()
                    .map(|effect| match effect {
                        Effect::Function(key, function) => {
                            if key == "test-key" {
                                // convert the function to task or thunk as you want, here we use task
                                return Effect::Task(Box::new(move || {
                                    let result = function();
                                    // result for 'test-key'should be Box<String>
                                    match result {
                                        Ok(result) => {
                                            let result_string: Box<String> =
                                                result.downcast().unwrap();
                                            println!("result_string: {:?}", result_string);
                                            assert_eq!(
                                                result_string.to_string(),
                                                "Result for test-key".to_string()
                                            );
                                        }
                                        Err(e) => {
                                            assert!(false, "result should be Ok: {:?}", e);
                                        }
                                    }
                                }));
                            } else {
                                return Effect::Function(key, function);
                            }
                        }
                        Effect::Action(action) => {
                            return Effect::Action(action);
                        }
                        Effect::Task(task) => {
                            return Effect::Task(task);
                        }
                        Effect::Thunk(thunk) => {
                            return Effect::Thunk(thunk);
                        }
                    })
                    .collect();

                if need_to_dispatch {
                    Ok(DispatchOp::Dispatch(state, new_effects))
                } else {
                    Ok(DispatchOp::Keep(state, new_effects))
                }
            })
        }
    }

    #[test]
    fn test_effect_function() {
        // Test Effect::Function - function that returns a result
        println!("Testing Effect::Function");

        let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
            .with_name("test-function-effect".into())
            .with_middleware(Arc::new(TestEffectMiddleware::new()))
            .build()
            .unwrap();

        let subscriber = Arc::new(TestSubscriber::default());
        store.add_subscriber(subscriber.clone()).unwrap();

        // Dispatch action that produces Effect::Function
        store.dispatch(TestAction::FunctionTask).unwrap();

        // Wait for effect to be processed
        thread::sleep(Duration::from_millis(100));

        // Stop store to ensure all effects are processed
        store.stop().unwrap();

        let actions = subscriber.get_actions();

        // Should have received the FunctionTask action
        assert!(actions.iter().any(|a| matches!(a, TestAction::FunctionTask)));

        println!("Effect::Function test passed");
    }

    #[test]
    fn test_effect_chain() {
        // Test chaining multiple effects
        println!("Testing Effect chaining");

        let store = StoreBuilder::new_with_reducer(TestState::default(), Box::new(TestReducer))
            .with_name("test-effect-chain".into())
            .build()
            .unwrap();

        let subscriber = Arc::new(TestSubscriber::default());
        store.add_subscriber(subscriber.clone()).unwrap();

        // Dispatch multiple actions with effects
        store.dispatch(TestAction::SetValue(5)).unwrap();
        store.dispatch(TestAction::ThunkTask(3)).unwrap();
        store.dispatch(TestAction::AsyncTask).unwrap();

        // Wait for all effects to be processed
        thread::sleep(Duration::from_millis(200));

        // Stop store to ensure all effects are processed
        store.stop().unwrap();

        let actions = subscriber.get_actions();

        // Should have multiple actions
        assert!(actions.len() >= 3);

        // Should have SetValue, ThunkTask, and AsyncTask
        assert!(actions.iter().any(|a| matches!(a, TestAction::SetValue(5))));
        assert!(actions.iter().any(|a| matches!(a, TestAction::ThunkTask(3))));
        assert!(actions.iter().any(|a| matches!(a, TestAction::AsyncTask)));

        // Should have AddValue from thunk
        assert!(actions.iter().any(|a| matches!(a, TestAction::AddValue(3))));

        // Should have AddMessage from SetValue effect
        assert!(actions.iter().any(|a| matches!(a, TestAction::AddMessage(_))));

        println!("Effect chaining test passed");
    }
}
