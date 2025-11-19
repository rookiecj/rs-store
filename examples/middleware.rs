use rs_store::{
    DispatchOp, FnReducer, MiddlewareContext, MiddlewareFn, MiddlewareFnFactory, StoreBuilder,
};
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Arc;
use std::time::Instant;

pub struct Metrics {
    pub action_count: AtomicUsize,
    pub action_executed: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            action_count: AtomicUsize::new(0),
            action_executed: AtomicU64::new(0),
        }
    }

    pub fn action_executed<Action>(
        &self,
        _action: Option<&Action>,
        _duration: std::time::Duration,
    ) {
        self.action_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.action_executed.fetch_add(_duration.as_secs(), std::sync::atomic::Ordering::Relaxed);
    }
}
/// Metrics middleware that tracks execution time and records metrics
/// This middleware wraps another middleware and records its execution metrics
///
#[allow(private_bounds)] // Metrics trait is pub(crate) but used internally
#[allow(dead_code)]
pub struct MetricsMiddleware<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    metrics: Arc<Metrics>,
    name: String,
    _phantom: std::marker::PhantomData<(State, Action)>,
}

#[allow(private_bounds)] // Metrics trait is pub(crate) but used internally
impl<State, Action> MetricsMiddleware<State, Action>
where
    State: Send + Sync + Clone,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    /// Create a new metrics middleware that wraps another middleware
    ///
    /// # Arguments
    /// * `inner` - The middleware to wrap and track
    /// * `metrics` - The metrics collector to use
    /// * `name` - The name of this middleware for metrics reporting
    pub fn new(name: String) -> Self {
        Self {
            metrics: Arc::new(Metrics::new()),
            name,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<State, Action> MiddlewareFnFactory<State, Action> for MetricsMiddleware<State, Action>
where
    State: Send + Sync + Clone + 'static,
    Action: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    fn create(&self, inner: MiddlewareFn<State, Action>) -> MiddlewareFn<State, Action> {
        let metrics = self.metrics.clone();
        let next = inner;
        let middleware: MiddlewareFn<State, Action> =
            Arc::new(move |ctx: &mut MiddlewareContext<State, Action>| {
                let start_time = Instant::now();

                // middleware executed
                // action executed
                let action = ctx.action.clone();
                // do more metrics here...

                // Call the next middleware in the chain
                let result = next(ctx);

                let duration = start_time.elapsed();

                // Record metrics
                metrics.action_executed(Some(&action), duration);

                println!(
                    "[MetricsMiddleware] Action executed: {:?}, Duration: {:?}",
                    action, duration
                );
                result
            });
        middleware
    }
}

pub fn main() {
    // Example usage of MetricsMiddleware
    let metrics_middleware: MetricsMiddleware<String, String> =
        MetricsMiddleware::new("example_metrics".to_string());
    let reducer = Box::new(FnReducer::from(|state: &String, action: &String| {
        let new_state = format!("{} + {}", state, action);
        DispatchOp::Dispatch(new_state, vec![])
    }));

    // Here you would integrate the middleware into your store builder
    // For example:
    let store_result = StoreBuilder::new("initial_state".to_string())
        .with_reducer(reducer)
        .with_middleware(Arc::new(metrics_middleware))
        .build();
    let store = store_result.unwrap();

    store.dispatch("action_1".to_string()).unwrap();

    store.stop().unwrap();
}
