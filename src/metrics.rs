use std::time::Duration;

#[allow(dead_code)]
pub trait StoreMetrics {
    fn action_processed(&self, action_type: &str, duration: Duration);
    fn state_changed(&self, old_size: usize, new_size: usize);
    fn subscriber_notified(&self, count: usize, duration: Duration);
}
