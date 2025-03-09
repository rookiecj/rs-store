use crate::metrics::Metrics;
use crate::ActionOp;
use crossbeam::channel::{self, Receiver, Sender, TrySendError};
use std::marker::PhantomData;
use std::sync::Arc;

/// the Backpressure policy
#[derive(Clone, Default)]
pub enum BackpressurePolicy {
    /// Block the sender when the queue is full
    #[default]
    BlockOnFull,
    /// Drop the oldest item when the queue is full
    DropOldest,
    /// Drop the latest item when the queue is full
    DropLatest,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SenderError<T> {
    #[error("Failed to send: {0}")]
    SendError(T),
    #[error("Failed to try_send: {0}")]
    TrySendError(TrySendError<T>),
}

/// Channel to hold the sender with backpressure policy
#[derive(Clone)]
pub(crate) struct SenderChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    _name: String,
    sender: Sender<ActionOp<T>>,
    receiver: Receiver<ActionOp<T>>,
    policy: BackpressurePolicy,
    metrics: Option<Arc<dyn Metrics + Send + Sync>>,
}

#[cfg(dev)]
impl<Action> Drop for SenderChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        eprintln!("store: drop '{}' sender channel", self._name);
    }
}

impl<T> SenderChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    pub fn send(&self, item: ActionOp<T>) -> Result<i64, SenderError<ActionOp<T>>> {
        let r = match self.policy {
            BackpressurePolicy::BlockOnFull => {
                match self.sender.send(item).map_err(|e| SenderError::SendError(e.0)) {
                    Ok(_) => Ok(self.receiver.len() as i64),
                    Err(e) => Err(e),
                }
            }
            BackpressurePolicy::DropOldest => {
                if let Err(TrySendError::Full(item)) = self.sender.try_send(item) {
                    // Drop the oldest item and try sending again
                    #[cfg(dev)]
                    eprintln!("store: dropping the oldest item in channel");
                    // Remove the oldest item
                    let _old = self.receiver.try_recv();
                    if let Some(metrics) = &self.metrics {
                        if let Ok(ActionOp::Action(action)) = _old.as_ref() {
                            metrics.action_dropped(Some(action));
                        }
                    }
                    match self.sender.try_send(item).map_err(SenderError::TrySendError) {
                        Ok(_) => Ok(self.receiver.len() as i64),
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(0)
                }
            }
            BackpressurePolicy::DropLatest => {
                // Try to send the item, if the queue is full, just ignore the item (drop the latest)
                match self.sender.try_send(item).map_err(SenderError::TrySendError) {
                    Ok(_) => Ok(self.receiver.len() as i64),
                    Err(err) => {
                        #[cfg(dev)]
                        eprintln!("store: dropping the latest item in channel");
                        if let Some(metrics) = &self.metrics {
                            if let SenderError::TrySendError(TrySendError::Full(
                                ActionOp::Action(action_drop),
                            )) = &err
                            {
                                metrics.action_dropped(Some(action_drop));
                            }
                        }
                        Err(err)
                    }
                }
            }
        };

        if let Some(metrics) = &self.metrics {
            metrics.queue_size(self.receiver.len());
        }
        r
    }
}

#[allow(dead_code)]
pub(crate) struct ReceiverChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    name: String,
    receiver: Receiver<ActionOp<T>>,
    metrics: Option<Arc<dyn Metrics + Send + Sync>>,
}

#[cfg(dev)]
impl<Action> Drop for ReceiverChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    fn drop(&mut self) {
        eprintln!("store: drop '{}' receiver channel", self.name);
    }
}

impl<T> ReceiverChannel<T>
where
    T: Send + Sync + Clone + 'static,
{
    pub fn recv(&self) -> Option<ActionOp<T>> {
        self.receiver.recv().ok()
    }

    #[allow(dead_code)]
    pub fn try_recv(&self) -> Option<ActionOp<T>> {
        self.receiver.try_recv().ok()
    }
}

/// Channel with back pressure
pub(crate) struct BackpressureChannel<MSG>
where
    MSG: Send + Sync + Clone + 'static,
{
    phantom_data: PhantomData<MSG>,
}

impl<MSG> BackpressureChannel<MSG>
where
    MSG: Send + Sync + Clone + 'static,
{
    #[allow(dead_code)]
    pub fn pair(
        capacity: usize,
        policy: BackpressurePolicy,
    ) -> (SenderChannel<MSG>, ReceiverChannel<MSG>) {
        Self::pair_with("<anon>", capacity, policy, None)
    }

    #[allow(dead_code)]
    pub fn pair_with_metrics(
        capacity: usize,
        policy: BackpressurePolicy,
        metrics: Option<Arc<dyn Metrics + Send + Sync>>,
    ) -> (SenderChannel<MSG>, ReceiverChannel<MSG>) {
        Self::pair_with("<anon>", capacity, policy, metrics)
    }

    #[allow(dead_code)]
    pub fn pair_with(
        name: &str,
        capacity: usize,
        policy: BackpressurePolicy,
        metrics: Option<Arc<dyn Metrics + Send + Sync>>,
    ) -> (SenderChannel<MSG>, ReceiverChannel<MSG>) {
        let (sender, receiver) = channel::bounded(capacity);
        (
            SenderChannel {
                _name: name.to_string(),
                sender,
                receiver: receiver.clone(),
                policy,
                metrics: metrics.clone(),
            },
            ReceiverChannel {
                name: name.to_string(),
                receiver,
                metrics: metrics.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_channel_backpressure_drop_old() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(5, BackpressurePolicy::DropOldest);

        let producer = {
            let sender_channel = sender.clone();
            thread::spawn(move || {
                for i in 0..20 {
                    // Send more messages than the channel can hold
                    println!("Sending: {}", i);
                    if let Err(err) = sender_channel.send(ActionOp::Action(i)) {
                        eprintln!("Failed to send: {:?}", err);
                    }
                    thread::sleep(Duration::from_millis(50)); // Slow down to observe full condition
                }
            })
        };

        let consumer = {
            thread::spawn(move || {
                let mut received_items = vec![];
                while let Some(value) = receiver.recv() {
                    println!("Received: {:?}", value);
                    match value {
                        ActionOp::Action(i) => received_items.push(i),
                        _ => {}
                    }
                    thread::sleep(Duration::from_millis(150)); // Slow down the consumer to create a backlog
                }
                println!("Channel closed, consumer thread exiting.");
                assert!(receiver.try_recv().is_none());

                received_items
            })
        };

        // Wait for the producer to finish
        producer.join().unwrap();
        drop(sender); // Close the channel after the producer is done

        // Collect the results from the consumer thread
        let received_items = consumer.join().unwrap();

        // Check the length of received items; it should be less than the total sent (20) due to drops
        assert!(received_items.len() < 20);
        // Ensure the last items were not dropped (based on the DropOld policy)
        assert_eq!(received_items.last(), Some(&19));
    }

    #[test]
    fn test_channel_backpressure_drop_latest() {
        let (sender, receiver) =
            BackpressureChannel::<i32>::pair(5, BackpressurePolicy::DropLatest);

        let producer = {
            let sender_channel = sender.clone();
            thread::spawn(move || {
                for i in 0..20 {
                    // Send more messages than the channel can hold
                    println!("Sending: {}", i);
                    if let Err(err) = sender_channel.send(ActionOp::Action(i)) {
                        eprintln!("Failed to send: {:?}", err);
                    }
                    thread::sleep(Duration::from_millis(50)); // Slow down to observe full condition
                }
            })
        };

        let consumer = {
            thread::spawn(move || {
                let mut received_items = vec![];
                while let Some(value) = receiver.recv() {
                    eprintln!("Received: {:?}", value);
                    match value {
                        ActionOp::Action(i) => received_items.push(i),
                        _ => {}
                    }
                    thread::sleep(Duration::from_millis(150)); // Slow down the consumer to create a backlog
                }
                println!("Channel closed, consumer thread exiting.");
                received_items
            })
        };

        // Wait for the producer to finish
        producer.join().unwrap();
        drop(sender); // Close the channel after the producer is done

        // Collect the results from the consumer thread
        let received_items = consumer.join().unwrap();

        // Check the length of received items; it should be less than the total sent (20) due to drops
        assert!(received_items.len() < 20);

        // Ensure the last item received is not necessarily the last one sent, based on the DropLatest policy
        assert!(received_items.contains(&0)); // The earliest items should be present
        assert!(received_items.last().unwrap() < &19); // The latest items might be dropped
    }
}
