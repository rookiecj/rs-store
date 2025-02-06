use crate::channel;
use crate::store::{StoreError, DEFAULT_CAPACITY};
use crate::BackpressurePolicy;
use crate::Middleware;
use crate::Reducer;
use crate::StoreImpl;
use std::sync::Arc;

pub struct StoreBuilder<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    name: String,
    state: State,
    reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
    without_reducer: bool,
    capacity: usize,
    policy: BackpressurePolicy,
    middlewares: Vec<Arc<dyn Middleware<State, Action> + Send + Sync>>,
}

impl<State, Action> StoreBuilder<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + 'static,
{
    /// Create a new store builder.
    pub fn new(state: State) -> Self {
        StoreBuilder {
            name: "store".to_string(),
            state,
            reducers: vec![],
            without_reducer: false,
            capacity: DEFAULT_CAPACITY,
            policy: Default::default(),
            middlewares: Vec::new(),
        }
    }

    /// Create a new store builder with a reducer.
    pub fn new_with_reducer(
        state: State,
        reducer: Box<dyn Reducer<State, Action> + Send + Sync>,
    ) -> Self {
        StoreBuilder {
            name: "store".to_string(),
            state,
            reducers: vec![reducer],
            without_reducer: false,
            capacity: DEFAULT_CAPACITY,
            policy: Default::default(),
            middlewares: Vec::new(),
        }
    }

    /// Set the name of the store.
    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    // /// Set the state of the store.
    // pub fn with_state(mut self, state: State) -> Self {
    //     self.state = state;
    //     self
    // }

    /// Set the reducer of the store.
    pub fn with_reducer(mut self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) -> Self {
        self.reducers = vec![reducer];
        self.without_reducer = false;
        self
    }

    /// Set the reducers of the store.
    pub fn with_reducers(
        mut self,
        reducers: Vec<Box<dyn Reducer<State, Action> + Send + Sync>>,
    ) -> Self {
        self.reducers = reducers;
        self.without_reducer = false;
        self
    }

    /// Add a reducer to the store.
    pub fn add_reducer(mut self, reducer: Box<dyn Reducer<State, Action> + Send + Sync>) -> Self {
        self.reducers.push(reducer);
        self
    }

    /// Create a store without reducers.
    pub fn without_reducer(mut self) -> Self {
        self.without_reducer = true;
        self
    }

    /// Set the capacity of the store.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self.without_reducer = false;
        self
    }

    /// Set the backpressure policy of the store.
    pub fn with_policy(mut self, policy: channel::BackpressurePolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Set the middleware of the store.
    pub fn with_middleware(
        mut self,
        middleware: Arc<dyn Middleware<State, Action> + Send + Sync>,
    ) -> Self {
        self.middlewares = vec![middleware];
        self
    }

    /// Set the middlewares of the store.
    pub fn with_middlewares(
        mut self,
        middlewares: Vec<Arc<dyn Middleware<State, Action> + Send + Sync>>,
    ) -> Self {
        self.middlewares = middlewares;
        self
    }

    /// Add a middleware to the store.
    pub fn add_middleware(
        mut self,
        middleware: Arc<dyn Middleware<State, Action> + Send + Sync>,
    ) -> Self {
        self.middlewares.push(middleware);
        self
    }

    /// Build a store impl.
    pub fn build(self) -> Result<Arc<StoreImpl<State, Action>>, StoreError> {
        if !self.without_reducer && self.reducers.is_empty() {
            return Err(StoreError::ReducerError("reducers are empty".to_string()));
        }
        if self.name.is_empty() {
            return Err(StoreError::ReducerError("name is empty".to_string()));
        }
        if self.capacity == 0 {
            return Err(StoreError::ReducerError("capacity is 0".to_string()));
        }

        StoreImpl::new_with(
            self.state,
            self.reducers,
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
    fn test_builder_without_reducer() {
        let store = StoreBuilder::<i32, i32>::new(0).without_reducer().build();
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

    #[test]
    fn test_builder_build_droppable() {
        let store = StoreBuilder::new(0).with_reducer(Box::new(TestReducer)).build();
        assert!(store.is_ok());
        drop(store);
    }
}
