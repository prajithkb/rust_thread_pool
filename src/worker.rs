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
use timed::timed_macro::timed_block;
/// A type that represents a  Callback whe the Worker Thread completes
/// Typically used to detect panic
pub(crate) type WorkerCallback =
    Arc<Mutex<dyn Fn(usize, Result<(), String>) -> () + Send + 'static>>;

/// Defines the Worker, this internally holds the handle to the thread that runs the Task
pub struct Worker {
    pub id: usize,
    thread: thread::JoinHandle<()>,
    receiver: Arc<Mutex<Receiver<Task>>>,
}

/// A 'drop' hook to detect Thread completion
struct ThreadFinishHook {
    id: usize,
    pub on_thread_complete: WorkerCallback,
    active_counter: Arc<AtomicUsize>,
}

impl Drop for ThreadFinishHook {
    fn drop(&mut self) {
        let callback = self.on_thread_complete.lock().unwrap();
        let mut result: Result<(), String> = Ok(());
        if thread::panicking() {
            // Decrement the counter only  if it panicked to accept new connections
            // We already decrement the count for non panic situation
            self.active_counter.fetch_sub(1, Ordering::SeqCst);
            result = Err("Panic!".to_string());
        }
        let status = result.clone().map_or("Panicked", |_| "Safe exit");
        guarded_println!("ThreadFinishHook: [Thread status: <{}>]", &status);
        // This callback removes this Worker from the queue
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
        pending_tasks: Arc<AtomicUsize>,
    ) -> Worker {
        timed_block!("Worker.new");
        guarded_println!("Creating a new worker with id {}", id);
        let receiver_inside = receiver.clone();
        let thread = thread::Builder::new()
            // .name(format!("Worker.Thread-{}", id))
            .spawn(move || {
                // un used variable that will get dropped if this thread panics
                // when the 'drop' happens the call back gets called
                let _panic_hook = ThreadFinishHook {
                    id,
                    on_thread_complete: on_complete,
                    active_counter: active_counter.clone(),
                };
                timed_block!("Thread.run");
                run(id, receiver_inside, active_counter, pending_tasks)();
            })
            .unwrap();

        Worker {
            id,
            thread,
            receiver,
        }
    }

    pub(crate) fn shutdown(self) {
        self.thread.join().unwrap();
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
    pending_tasks: Arc<AtomicUsize>,
) -> Runnable {
    timed_block!("Worker.run");
    let receiver_inside = receiver.clone();
    let counter = active_counter.clone();
    let p = pending_tasks.clone();
    Box::new(move || loop {
        guarded_println!("Worker-{}, Waiting...", id);
        // Only lock jobs for the time it takes
        // to get a job, not run it.
        let pending_job = receiver_inside.lock().unwrap().recv();
        match pending_job {
            Ok(task) => {
                let task_id = task.id;
                guarded_println!("Worker-{}, received Job {}", id, task_id);
                (task.runnable)();
                guarded_println!("Worker-{}, finished Job {}", id, task_id);
                counter.fetch_sub(1, Ordering::SeqCst);
                // Need to make this atomic with the one above TODO
                p.fetch_sub(1, Ordering::SeqCst);
            }
            Err(_) => {
                guarded_println!("Worker-{}, Channel closed", id);
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::Worker;
    use crate::task::Task;
    use crate::worker::WorkerCallback;
    use core::sync::atomic::AtomicUsize;
    use mpsc::Receiver;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::Sender;
    use std::thread;
    use std::time::Duration;
    use std::{
        sync::Arc,
        sync::{mpsc, Mutex},
    };

    #[test]
    fn runs_task() -> Result<(), String> {
        let id = 1;
        let (sender, receiver): (Sender<Task>, Receiver<Task>) = mpsc::channel();
        let receiver: Arc<Mutex<Receiver<Task>>> = Arc::new(Mutex::new(receiver));
        let (on_complete_sender, on_complete_receiver): (Sender<String>, Receiver<String>) =
            mpsc::channel();
        let c = move |_: usize, b: Result<(), String>| {
            if b.is_ok() {
                on_complete_sender.send("Success".to_string())
            } else {
                on_complete_sender.send("Failure".to_string())
            }
            .unwrap();
        };
        let on_complete: WorkerCallback = Arc::new(Mutex::new(c));
        let active_counter = Arc::new(AtomicUsize::new(1));
        let pending_tasks = Arc::new(AtomicUsize::new(1));
        let _worker = Worker::new(
            id,
            receiver,
            on_complete,
            active_counter.clone(),
            pending_tasks,
        );
        let (test_sender, test_receiver) = mpsc::channel();
        let task = Task::new(
            Box::new(move || {
                test_sender.send("Hi").unwrap();
                thread::sleep(Duration::from_millis(100));
            }),
            id.to_string(),
        );
        sender.send(task).unwrap();
        // Assert that task is run
        assert_eq!(
            "Hi",
            test_receiver
                .recv_timeout(Duration::from_millis(100))
                .map_err(|_| { "Worker failed to run" })?
        );
        assert_eq!(1, active_counter.load(Ordering::SeqCst));
        drop(sender);
        // Assert that it is a safe exit
        assert_eq!(
            "Success",
            on_complete_receiver
                .recv_timeout(Duration::from_millis(500))
                .map_err(|_| { "Worker failed to notify on_complete" })?
        );
        // Assert that the active counter is reset to 0
        assert_eq!(0, active_counter.load(Ordering::SeqCst));
        Ok(())
    }

    #[test]
    fn invokes_callack_on_panic() -> Result<(), String> {
        let id = 1;
        let (sender, receiver): (Sender<Task>, Receiver<Task>) = mpsc::channel();
        let receiver: Arc<Mutex<Receiver<Task>>> = Arc::new(Mutex::new(receiver));
        let (on_complete_sender, on_complete_receiver): (Sender<String>, Receiver<String>) =
            mpsc::channel();
        let c = move |_: usize, b: Result<(), String>| {
            if b.is_ok() {
                on_complete_sender.send("Success".to_string())
            } else {
                on_complete_sender.send("Failure".to_string())
            }
            .unwrap();
        };
        let on_complete: WorkerCallback = Arc::new(Mutex::new(c));
        let active_counter = Arc::new(AtomicUsize::new(1));
        let pending_tasks = Arc::new(AtomicUsize::new(1));
        let _worker = Worker::new(
            id,
            receiver,
            on_complete,
            active_counter.clone(),
            pending_tasks,
        );
        let task = Task::new(
            Box::new(move || {
                thread::sleep(Duration::from_millis(100));
                panic!();
            }),
            id.to_string(),
        );
        sender.send(task).unwrap();
        drop(sender);
        // Assert that it is a safe exit
        assert_eq!(
            "Failure",
            on_complete_receiver
                .recv_timeout(Duration::from_millis(500))
                .map_err(|_| { "Worker failed to notify on_complete" })?
        );
        // Assert that the active counter is reset to 0
        assert_eq!(0, active_counter.load(Ordering::SeqCst));
        Ok(())
    }
}
