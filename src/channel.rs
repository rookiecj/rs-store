use crossbeam::channel::{self, Receiver, Sender, TrySendError};
use std::marker::PhantomData;
use std::time::Duration;
use std::{fmt, thread};

/// the Backpressure policy
#[derive(Clone, Copy)]
pub enum BackpressurePolicy {
    /// Drop the oldest item when the queue is full
    DropOld,
    /// Drop the latest item when the queue is full
    DropLatest,
}

/// Channel to hold the sender with backpressure policy
#[derive(Clone)]
pub struct SenderChannel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
    policy: BackpressurePolicy,
}

impl<T> SenderChannel<T> {
    pub fn new(sender: Sender<T>, receiver: Receiver<T>, policy: BackpressurePolicy) -> Self {
        SenderChannel {
            sender,
            receiver,
            policy,
        }
    }

    pub fn send(&self, item: T) -> Result<(), TrySendError<T>> {
        match self.policy {
            BackpressurePolicy::DropOld => {
                if let Err(TrySendError::Full(item)) = self.sender.try_send(item) {
                    // Drop the oldest item and try sending again
                    let _ = self.receiver.try_recv(); // Remove the oldest item
                    self.sender.try_send(item)
                } else {
                    Ok(())
                }
            }
            BackpressurePolicy::DropLatest => {
                // Try to send the item, if the queue is full, just ignore the item (drop the latest)
                self.sender.try_send(item)
            }
        }
    }
}

pub struct ReceiverChannel<T> {
    receiver: Receiver<T>,
}

impl<T> ReceiverChannel<T> {
    pub fn recv(&self) -> Option<T> {
        self.receiver.recv().ok()
    }

    pub fn try_recv(&self) -> Option<T> {
        self.receiver.try_recv().ok()
    }
}

/// Channel with back pressure
pub(crate) struct BackpressureChannel<T> {
    capacity: usize,
    policy: BackpressurePolicy,
    phantom_data: PhantomData<T>,
}

impl<T> BackpressureChannel<T> {
    pub fn new(
        capacity: usize,
        policy: BackpressurePolicy,
    ) -> (SenderChannel<T>, ReceiverChannel<T>) {
        let (sender, receiver) = channel::bounded(capacity);
        (
            SenderChannel {
                sender,
                receiver: receiver.clone(),
                policy,
            },
            ReceiverChannel { receiver },
        )
    }
}

fn main() {
    let (sender, receiver) = BackpressureChannel::new(5, BackpressurePolicy::DropOld);

    let mut producers = vec![];

    for i in 0..10 {
        let sender_channel = sender.clone();
        let producer = thread::spawn(move || {
            for j in 0..10 {
                let item = i * 10 + j; // Unique value per producer
                println!("Producer {} sending: {}", i, item);
                if let Err(err) = sender_channel.send(item) {
                    eprintln!("Producer {} failed to send: {:?}", i, err);
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        producers.push(producer);
    }

    let consumer = {
        thread::spawn(move || {
            while let Some(value) = receiver.recv() {
                println!("Received: {}", value);
            }
            println!("Channel closed, consumer thread exiting.");
        })
    };

    for producer in producers {
        producer.join().unwrap();
    }

    drop(sender); // Close the channel after all producers are done
    consumer.join().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn test_channel_backpressure_drop_old() {
        let (sender, receiver) = BackpressureChannel::new(5, BackpressurePolicy::DropOld);

        let (tx, rx) = mpsc::channel(); // Channel to signal when the producer is done

        let producer = {
            let sender_channel = sender.clone();
            thread::spawn(move || {
                for i in 0..20 {
                    // Send more messages than the channel can hold
                    println!("Sending: {}", i);
                    if let Err(err) = sender_channel.send(i) {
                        eprintln!("Failed to send: {:?}", err);
                    }
                    thread::sleep(Duration::from_millis(50)); // Slow down to observe full condition
                }
                tx.send(()).unwrap(); // Signal that the producer is done
            })
        };

        let consumer = {
            thread::spawn(move || {
                let mut received_items = vec![];
                while let Some(value) = receiver.recv() {
                    println!("Received: {}", value);
                    received_items.push(value);
                    thread::sleep(Duration::from_millis(150)); // Slow down the consumer to create a backlog
                }
                println!("Channel closed, consumer thread exiting.");
                assert_eq!(receiver.try_recv(), None);

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

        // Check that the channel is closed
        // assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

        assert_eq!(rx.try_recv(), Ok(()));
    }

    #[test]
    fn test_channel_backpressure_drop_latest() {
        let (sender, receiver) = BackpressureChannel::new(5, BackpressurePolicy::DropLatest);

        let (tx, rx) = mpsc::channel(); // Channel to signal when the producer is done

        let producer = {
            let sender_channel = sender.clone();
            thread::spawn(move || {
                for i in 0..20 {
                    // Send more messages than the channel can hold
                    println!("Sending: {}", i);
                    if let Err(err) = sender_channel.send(i) {
                        eprintln!("Failed to send: {:?}", err);
                    }
                    thread::sleep(Duration::from_millis(50)); // Slow down to observe full condition
                }
                tx.send(()).unwrap(); // Signal that the producer is done
            })
        };

        let consumer = {
            thread::spawn(move || {
                let mut received_items = vec![];
                while let Some(value) = receiver.recv() {
                    println!("Received: {}", value);
                    received_items.push(value);
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

        // Check that the channel is closed
        // assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }
}
