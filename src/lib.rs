use bus::BusReader;
use crossbeam_channel::SendError;
use std::{cell::RefCell, sync::{Arc, Mutex}, thread, time::Duration};
pub use bus::BusIter;

pub struct Bus<T> {
    bus: Arc<Mutex<RefCell<bus::Bus<T>>>>,
    sx_broadcast: crossbeam_channel::Sender<T>,
}

impl<T> Bus<T>
where
    T: Send + 'static,
{
    pub fn new(len: usize) -> Self {
        let (sx_broadcast, rx_broadcast) = crossbeam_channel::unbounded::<T>();

        let bus = Arc::new(Mutex::new(RefCell::new(bus::Bus::new(len))));
        let thread_bus = bus.clone();

        thread::spawn(move || {
            for message in rx_broadcast.iter() {
                thread_bus
                    .lock()
                    .expect("unable to obtain lock on bus")
                    .borrow_mut()
                    .broadcast(message);
            }

            // will terminate when the sx_broadcast in `Bus` gets dropped
        });

        Self { bus, sx_broadcast }
    }

    pub fn subscribe(&self) -> Receiver<T> {
        let bus = self.bus.lock().expect("unable to aqcuire lock on bus");
        let bus_reader = bus.borrow_mut().add_rx();

        Receiver {
            bus: self.bus.clone(),
            bus_reader,
        }
    }

    pub fn broadcast(&self, message: T) -> Result<(), SendError<T>> {
        self.sx_broadcast.send(message)
    }

    pub fn broadcaster(&self) -> Sender<T> {
        Sender {
            sx_broadcast: self.sx_broadcast.clone(),
        }
    }
}

pub struct Receiver<T> {
    bus: Arc<Mutex<RefCell<bus::Bus<T>>>>,
    bus_reader: BusReader<T>,
}

pub use std::sync::mpsc::{RecvError, RecvTimeoutError, TryRecvError};

impl<T> Receiver<T>
where
    T: Clone + Sync,
{
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.bus_reader.try_recv()
    }

    pub fn recv(&mut self) -> Result<T, RecvError> {
        self.bus_reader.recv()
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        self.bus_reader.recv_timeout(timeout)
    }

    pub fn iter(&mut self) -> BusIter<'_, T> {
        self.bus_reader.iter()
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let bus = self.bus.lock().expect("unable to obtain lock on bus");
        let bus_reader = bus.borrow_mut().add_rx();

        Self {
            bus: self.bus.clone(),
            bus_reader,
        }
    }
}

pub struct Sender<T> {
    sx_broadcast: crossbeam_channel::Sender<T>,
}

impl<T> Sender<T> {
    pub fn broadcast(&self, message: T) -> Result<(), SendError<T>> {
        self.sx_broadcast.send(message)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            sx_broadcast: self.sx_broadcast.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Bus;
    use std::{
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
        thread, time::Duration,
    };

    #[test]
    fn test_broadcast_single_threaded() {
        let bus: Bus<u32> = Bus::new(1000);

        let mut rx = bus.subscribe();
        bus.broadcast(10).unwrap();
        let message = rx.recv().unwrap();
        assert_eq!(10, message);

        let sx = bus.broadcaster();
        sx.broadcast(20).unwrap();

        let message = rx.recv().unwrap();
        assert_eq!(20, message);
    }

    #[test]
    fn test_broadcast_multi_threaded() {
        let bus: Bus<u32> = Bus::new(1000);

        let mut rx1 = bus.subscribe();
        let mut rx2 = rx1.clone();

        // this clone cannot happen inside thread2 because
        // otherwise the message might be broadcast before
        // it is created, and therefore it will not recieve
        // the message.
        let mut rx3 = rx1.clone();

        let result1: Arc<AtomicU32> = Arc::new(AtomicU32::new(0));
        let result2: Arc<AtomicU32> = Arc::new(AtomicU32::new(0));
        let result3: Arc<AtomicU32> = Arc::new(AtomicU32::new(0));

        let thread_result1 = result1.clone();
        let thread_result3 = result3.clone();
        let join_handle1 = thread::spawn(move || {
            let join_handle3 = thread::spawn(move || {
                loop {
                    let message = rx3.recv_timeout(Duration::from_millis(1000)).expect("thread3 did not receive any messages");
                    match message {
                        100 => break,
                        _ => {
                            thread_result3.store(message * 3, Ordering::SeqCst);
                        }
                    }
                }
            });

            while let Ok(message) = rx1.recv() {
                match message {
                    100 => break,
                    _ => thread_result1.store(message, Ordering::SeqCst),
                }
            }

            join_handle3.join().unwrap();
        });

        let thread_result2 = result2.clone();
        let join_handle2 = thread::spawn(move || {
            while let Ok(message) = rx2.recv() {
                match message {
                    100 => return,
                    _ => thread_result2.store(message * 2, Ordering::SeqCst),
                }
            }
        });

        bus.broadcast(10).unwrap();
        bus.broadcast(100).unwrap();

        join_handle1.join().unwrap();
        join_handle2.join().unwrap();

        assert_eq!(10u32, result1.load(Ordering::SeqCst));
        assert_eq!(20u32, result2.load(Ordering::SeqCst));
        assert_eq!(30u32, result3.load(Ordering::SeqCst));
    }
}
