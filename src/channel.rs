use crossbeam::channel::{self, Receiver, Sender, TrySendError};
use std::marker::PhantomData;
use std::sync::Arc;
use crate::metrics::StoreMetrics;
use crate::ActionOp;

/// the Backpressure policy
#[derive(Clone, Copy, Default)]
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
pub(crate) struct SenderChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    sender: Sender<ActionOp<Action>>,
    receiver: Receiver<ActionOp<Action>>,
    policy: BackpressurePolicy,
    metrics: Option<Arc<dyn StoreMetrics + Send + Sync>>,
}

impl<Action> SenderChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    pub fn send(&self, item: ActionOp<Action>) -> Result<i64, SenderError<ActionOp<Action>>> {
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
                        match _old.as_ref() {
                            Ok(ActionOp::Action(action)) => metrics.action_dropped(Some(action)),
                            _ => {}
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
                    Err(e) => {
                        #[cfg(dev)]
                        eprintln!("store: dropping the latest item in channel");
                        if let Some(metrics) = &self.metrics {
                            match &e {
                                SenderError::TrySendError(inner_err) => match inner_err {
                                    TrySendError::Full(item) => match item {
                                        ActionOp::Action(action) => {
                                            metrics.action_dropped(Some(action))
                                        }
                                        _ => {}
                                    },
                                    _ => {}
                                },
                                _ => {}
                            }
                        }
                        Err(e)
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
pub(crate) struct ReceiverChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    receiver: Receiver<ActionOp<Action>>,
    metrics: Option<Arc<dyn StoreMetrics + Send + Sync>>,
}

impl<Action> ReceiverChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    pub fn recv(&self) -> Option<ActionOp<Action>> {
        self.receiver.recv().ok()
    }

    #[allow(dead_code)]
    pub fn try_recv(&self) -> Option<ActionOp<Action>> {
        self.receiver.try_recv().ok()
    }
}

/// Channel with back pressure
pub(crate) struct BackpressureChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    phantom_data: PhantomData<Action>,
}

impl<Action> BackpressureChannel<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    #[allow(dead_code)]
    pub fn pair(
        capacity: usize,
        policy: BackpressurePolicy,
    ) -> (SenderChannel<Action>, ReceiverChannel<Action>) {
        Self::pair_with_metrics(capacity, policy, None)
    }

    pub fn pair_with_metrics(
        capacity: usize,
        policy: BackpressurePolicy,
        metrics: Option<Arc<dyn StoreMetrics + Send + Sync>>,
    ) -> (SenderChannel<Action>, ReceiverChannel<Action>) {
        let (sender, receiver) = channel::bounded(capacity);
        (
            SenderChannel {
                sender,
                receiver: receiver.clone(),
                policy,
                metrics: metrics.clone(),
            },
            ReceiverChannel {
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
                    println!("Received: {:?}", value);
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
