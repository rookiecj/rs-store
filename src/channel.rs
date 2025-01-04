use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crossbeam::channel::{self, Receiver, Sender, TrySendError};

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
pub(crate) struct SenderChannel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
    policy: BackpressurePolicy,
}

impl<T> SenderChannel<T> {
    // pub fn new(sender: Sender<T>, receiver: Receiver<T>, policy: BackpressurePolicy) -> Self {
    //     SenderChannel {
    //         sender,
    //         receiver,
    //         policy,
    //     }
    // }

    pub fn send(&self, item: T) -> Result<i64, SenderError<T>> {
        match self.policy {
            BackpressurePolicy::BlockOnFull => {
                match self.sender.send(item).map_err(|e| SenderError::SendError(e.0)) {
                    Ok(_) => Ok(self.receiver.len() as i64),
                    Err(e) => Err(e),
                }
            }
            BackpressurePolicy::DropOldest => {
                if let Err(TrySendError::Full(item)) = self.sender.try_send(item) {
                    // Drop the oldest item and try sending again
                    #[cfg(feature = "dbg")]
                    eprintln!("store: dropping the oldest item in channel");
                    let _ = self.receiver.try_recv(); // Remove the oldest item
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
                        #[cfg(feature = "dbg")]
                        eprintln!("store: dropping the latest item in channel");
                        Err(e)
                    }
                }
            }
        }
    }
}

pub(crate) struct ReceiverChannel<T> {
    receiver: Receiver<T>,
    pending: Mutex<VecDeque<T>>,
    capacity: usize,
    policy: BackpressurePolicy,
}

impl<T> ReceiverChannel<T>
where
    T: Clone + HasPriority,
{
    pub fn new(receiver: Receiver<T>, capacity: usize, policy: BackpressurePolicy) -> Self {
        Self {
            receiver,
            pending: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            policy,
        }
    }

    pub fn recv(&self) -> Option<T> {
        let mut pending = self.pending.lock().unwrap();

        // 새로운 메시지들을 pending queue에 추가 (capacity 제한 적용)
        while let Ok(item) = self.receiver.try_recv() {
            if pending.len() >= self.capacity {
                match self.policy {
                    BackpressurePolicy::DropOldest => {
                        // 가장 낮은 우선순위의 항목을 제거
                        let mut lowest_prio_idx = 0;
                        let mut lowest_prio = pending[0].priority();

                        for (i, item) in pending.iter().enumerate().skip(1) {
                            let prio = item.priority();
                            if prio < lowest_prio {
                                lowest_prio = prio;
                                lowest_prio_idx = i;
                            }
                        }
                        pending.remove(lowest_prio_idx);
                    }
                    BackpressurePolicy::DropLatest => {
                        // 새로운 항목을 무시
                        continue;
                    }
                    BackpressurePolicy::BlockOnFull => {
                        // 이 경우는 발생하지 않아야 함 (sender에서 이미 처리)
                        break;
                    }
                }
            }
            pending.push_back(item);
        }

        // pending queue가 비어있으면 blocking recv 시도
        if pending.is_empty() {
            match self.receiver.recv() {
                Ok(item) => Some(item),
                Err(_) => None,
            }
        } else {
            // 가장 높은 우선순위를 가진 항목을 찾아서 처리
            let mut highest_prio_idx = 0;
            let mut highest_prio = pending[0].priority();

            for (i, item) in pending.iter().enumerate().skip(1) {
                let prio = item.priority();
                if prio > highest_prio {
                    highest_prio = prio;
                    highest_prio_idx = i;
                }
            }

            Some(pending.remove(highest_prio_idx).unwrap())
        }
    }
}

// 우선순위를 가진 타입을 위한 trait
pub trait HasPriority {
    fn priority(&self) -> i32;
}

// ActionOp에 대한 HasPriority 구현
impl<Action> HasPriority for ActionOp<Action>
where
    Action: Send + Sync + Clone + 'static,
{
    fn priority(&self) -> i32 {
        self.priority()
    }
}

/// Channel with back pressure
pub(crate) struct BackpressureChannel<T> {
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
            ReceiverChannel::new(receiver, capacity, policy),
        )
    }
}
#[allow(dead_code)]
fn main() {
    let (sender, receiver) = BackpressureChannel::new(5, BackpressurePolicy::DropOldest);

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

    #[test]
    fn test_channel_backpressure_drop_old() {
        let (sender, receiver) = BackpressureChannel::new(5, BackpressurePolicy::DropOldest);

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
    }

    #[test]
    fn test_channel_backpressure_drop_latest() {
        let (sender, receiver) = BackpressureChannel::new(5, BackpressurePolicy::DropLatest);

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
    }
}
