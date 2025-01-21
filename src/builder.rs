use crate::channel;
use crate::BackpressurePolicy;
use crate::Middleware;
use crate::Reducer;
use crate::Store;
use crate::StoreError;
use crate::DEFAULT_CAPACITY;
use std::sync::Arc;

pub struct StoreBuilder<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    name: String,
    state: State,
    reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
    capacity: usize,
    policy: BackpressurePolicy,
    middlewares: Vec<Arc<dyn Middleware<State, Action> + Send + Sync>>,
}

impl<State, Action> StoreBuilder<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    pub fn new(state: State) -> Self {
        StoreBuilder {
            name: "store".to_string(),
            state,
            reducers: vec![],
            capacity: DEFAULT_CAPACITY,
            policy: Default::default(),
            middlewares: Vec::new(),
        }
    }

    pub fn new_with_reducer(
        state: State,
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
    ) -> Self {
        StoreBuilder {
            name: "store".to_string(),
            state,
            reducers: vec![reducer],
            capacity: DEFAULT_CAPACITY,
            policy: Default::default(),
            middlewares: Vec::new(),
        }
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    pub fn with_reducer(mut self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) -> Self {
        self.reducers = vec![reducer];
        self
    }

    pub fn with_reducers(
        mut self,
        reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
    ) -> Self {
        self.reducers = reducers;
        self
    }

    pub fn add_reducer(mut self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) -> Self {
        self.reducers.push(reducer);
        self
    }

    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn with_policy(mut self, policy: channel::BackpressurePolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_middleware(
        mut self,
        middleware: Arc<dyn Middleware<State, Action> + Send + Sync>,
    ) -> Self {
        self.middlewares = vec![middleware];
        self
    }

    pub fn with_middlewares(
        mut self,
        middlewares: Vec<Arc<dyn Middleware<State, Action> + Send + Sync>>,
    ) -> Self {
        self.middlewares = middlewares;
        self
    }

    pub fn add_middleware(
        mut self,
        middleware: Arc<dyn Middleware<State, Action> + Send + Sync>,
    ) -> Self {
        self.middlewares.push(middleware);
        self
    }

    pub fn build(self) -> Result<Arc<Store<State, Action>>, StoreError> {
        if self.reducers.is_empty() {
            return Err(StoreError::ReducerError("reducers are empty".to_string()));
        }
        if self.name.is_empty() {
            return Err(StoreError::ReducerError("name is empty".to_string()));
        }
        if self.capacity == 0 {
            return Err(StoreError::ReducerError("capacity is 0".to_string()));
        }

        Store::new_with(
            self.reducers,
            self.state,
            self.name,
            self.capacity,
            self.policy,
            self.middlewares,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{DispatchOp, Dispatcher, MiddlewareOp};

    use super::*;

    struct TestReducer;
    //     Effect: Fn(Box<dyn Dispatcher<Action>>) + Send + Sync + 'static,
    impl Reducer<i32, i32> for TestReducer {
        fn reduce(&self, state: &i32, action: &i32) -> DispatchOp<i32, i32> {
            let new_state = state + action;
            DispatchOp::Dispatch(new_state, None)
        }
    }

    #[test]
    fn test_builder() {
        let store = StoreBuilder::new(0).with_reducer(Box::new(TestReducer)).build();
        assert!(store.is_ok());
    }

    #[test]
    fn test_builder_with_name() {
        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(TestReducer))
            .with_name("test".to_string())
            .build();
        assert!(store.is_ok());
    }

    #[test]
    fn test_builder_with_reducers() {
        let store = StoreBuilder::new(0)
            .with_reducers(vec![Box::new(TestReducer), Box::new(TestReducer)])
            .build();
        assert!(store.is_ok());
    }

    #[test]
    fn test_builder_with_capacity() {
        let store =
            StoreBuilder::new(0).with_reducer(Box::new(TestReducer)).with_capacity(100).build();
        assert!(store.is_ok());
    }

    struct TestMiddleware;
    impl Middleware<i32, i32> for TestMiddleware {
        fn before_reduce(
            &self,
            _action: &i32,
            _state: &i32,
            _dispatcher: Arc<dyn Dispatcher<i32>>,
        ) -> Result<MiddlewareOp, StoreError> {
            Ok(MiddlewareOp::ContinueAction)
        }
        fn before_effect(
            &self,
            _action: &i32,
            _state: &i32,
            _effects: &mut Vec<crate::Effect<i32>>,
            _dispatcher: Arc<dyn Dispatcher<i32>>,
        ) -> Result<MiddlewareOp, StoreError> {
            Ok(MiddlewareOp::ContinueAction)
        }
    }

    #[test]
    fn test_builder_with_middleware() {
        let store = StoreBuilder::new(0)
            .with_reducer(Box::new(TestReducer))
            .with_middleware(Arc::new(TestMiddleware))
            .build();
        assert!(store.is_ok());
    }
}
