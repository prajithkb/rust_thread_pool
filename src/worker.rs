use crate::timed_execution::log_time;
use crate::{task::Task, thread_pool::Runnable};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use mpsc::Receiver;
use std::fmt::Debug;
use std::{
    sync::Arc,
    sync::{mpsc, Mutex},
    thread,
};
/// A type that represents a  Callback whe the Worker Thread completes
/// Typically used to detect panic
pub(crate) type WorkerCallback =
    Arc<Mutex<dyn Fn(usize, Result<(), String>) -> () + Send + Sync + 'static>>;

/// Defines the Worker, this internally holds the handle to the thread that runs the Task
pub struct Worker {
    pub id: usize,
    thread: thread::JoinHandle<()>,
    receiver: Arc<Mutex<Receiver<Task>>>
}

/// A 'drop-on-panic' hook to detect panic
struct PanicHook {
    id: usize,
    pub on_thread_complete: WorkerCallback,
}

impl Drop for PanicHook {
    fn drop(&mut self) {
        let callback = self.on_thread_complete.lock().unwrap();
        let mut result: Result<(), String> = Ok(());
        if thread::panicking() {
            result = Err("Panic!".to_string());
        }
        let status = result.clone().map_or("Panic", |_| "Safe exit");
        println!("PanicHook: Thread status {}", &status);
        (callback)(self.id, result);
    }
}

impl Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Worker")
            .field("id", &self.id)
            .field("thread", &self.thread)
            .field("receiver", &self.receiver)
            .field("on_thread_complete", &"WorkerCallback")
            .finish()
    }
}

impl Worker {
    
    /// Creates a new instance of the Worker
    /// 
    /// This uses the `receiver` to listen for Tasks
    /// The `on_thread_complete` WorkerCallback is invoked to notify the callbacks
    pub(crate) fn new(
        id: usize,
        receiver: Arc<Mutex<Receiver<Task>>>,
        on_complete: WorkerCallback,
        active_counter: Arc<AtomicUsize>,
    ) -> Worker {
        let _log_time = log_time("Worker.new");
        println!("Creating a new worker with id {}", id);
        let receiver_inside = receiver.clone();
        let thread = thread::spawn(move || {
            // un used variable that will get dropped if this thread panics
            // when the 'drop' happens the call back gets called
            let _panic_hook = PanicHook {
                id,
                on_thread_complete: on_complete,
            };
            let _log_time = log_time("Thread.run"); 
            run(id, receiver_inside, active_counter)();
        });

        Worker {
            id,
            thread,
            receiver,
        }
    }
}

/// The main Worker loop.
/// 
/// This method loops infinitely, reading from the receiver channel
/// 
/// For every task it recieves from the channel, it increments and decrements the atomic counter
/// before and after
fn run(
    id: usize,
    receiver: Arc<Mutex<Receiver<Task>>>,
    active_counter: Arc<AtomicUsize>,
) -> Runnable {
    let _log_time = log_time("Worker.run");
    let receiver_inside = receiver.clone();
    let counter = active_counter.clone();
    Box::new(move || loop {
        println!("Worker-{}, Waiting...", id);
        let job = receiver_inside.lock().unwrap().recv().unwrap();
        counter.fetch_add(1, Ordering::SeqCst);
        println!("Worker-{}, received Job {:?}", id, job);
        counter.fetch_sub(1, Ordering::SeqCst);
        (job.runnable)();
       
    })
}

