use core::sync::atomic::AtomicBool;
use crate::task::Task;
use crate::{
    worker::{Worker, WorkerCallback},
};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use mpsc::{Receiver, Sender};
use std::{
    cell::RefCell,
    sync::Arc,
    sync::{mpsc, Mutex},
};

/// Defines the error states returned by `thread_pool.execute`
#[derive(Debug, PartialEq)]
pub enum ExecutionError {
    /// No threads are available to perform this task
    NoThreadAvailable,
    /// The internal channel for submitting the tasks failed
    TaskChannelError,
    /// if the thread pool is shutdown
    Shutdown,
}

/// A runnable function, declared as a type alias
pub type Runnable = Box<dyn FnOnce() + Send + 'static>;
/// A simple fixed size Thread pool, that has a manages a set of 'workers' to execute [`Runnable`][Runnable] types
///
/// [Runnable]: ./ThreadPool.html#type.Runnable  
pub struct ThreadPool {
    workers: Arc<Mutex<Vec<Worker>>>,
    sender: RefCell<Box<Option<Sender<Task>>>>,
    number_active_workers: Arc<AtomicUsize>,
    maximum_number_of_threads: usize,
    task_queue: Arc<Mutex<Receiver<Task>>>,
    worker_callback: WorkerCallback,
    pending_tasks: Arc<AtomicUsize>,
    shutdown_initiated: Arc<AtomicBool>
}

impl ThreadPool {
    /// Creates an instance of a [`Runnable`][Runnable]
    /// Creates an fixed size ThreadPool.
    /// ## Arguments
    /// * `maximum_number_of_threads` - The maximum number of threads that is allowed.
    ///     This number cannot be more than 50
    ///
    /// ## Examples
    /// ```ignore
    /// let thread_pool = ThreadPool::new(10)
    /// ```
    /// [Runnable]: ./ThreadPool.html#type.Runnable  
    pub fn new(maximum_number_of_threads: usize) -> ThreadPool {
        timed!("ThreadPool.new");
        assert!(maximum_number_of_threads > 0 && maximum_number_of_threads < 50);
        let number_active_workers = Arc::new(AtomicUsize::new(0));
        let pending_tasks = Arc::new(AtomicUsize::new(0));
        let (sender, r): (Sender<Task>, Receiver<Task>) = mpsc::channel();
        let task_queue: Arc<Mutex<Receiver<Task>>> = Arc::new(Mutex::new(r));
        let c = Arc::new(AtomicBool::new(false));
        let b = c.clone();
        let workers: Arc<Mutex<Vec<Worker>>> =
            Arc::new(Mutex::new(Vec::with_capacity(maximum_number_of_threads)));
        let workers_for_callback = workers.clone();
        let w: WorkerCallback = Arc::new(Mutex::new(move |id, result: Result<(), String>| {
            if !b.load(Ordering::SeqCst) {
                println!("Removing worker, status [{:?}]", result);
                let mut w = workers_for_callback.lock().unwrap();
                w.remove(id);
                println!("Number of workers {}", w.len());
            } else {
                println!("Shutdown initiated, nothing to do for Worker {}", id);
            }
        }));
        for id in 0..maximum_number_of_threads {
            let remove_item: WorkerCallback = w.clone();
            let worker = Worker::new(
                id,
                task_queue.clone(),
                remove_item,
                number_active_workers.clone(),
                pending_tasks.clone()

            );
            workers.lock().unwrap().push(worker);
        }
        let thread_pool = ThreadPool {
            workers,
            sender: RefCell::new(Box::new(Some(sender))),
            number_active_workers,
            maximum_number_of_threads,
            task_queue,
            worker_callback: w,
            pending_tasks,
            shutdown_initiated: c
        };
        thread_pool
    }

    /// Initiates a shuts down the Thread pool
    /// All the Workers are notified. This does not stop any ongoing tasks.
    /// Potential memory leak, we need to drop the channel
    /// TODO: Implement shutdown_immediately()?
    fn init_shutdown(&self) {
        self.shutdown_initiated.store(true, Ordering::SeqCst);
        let mut c = self.sender.borrow_mut();
        if c.is_none() {
            println!("Threadpool shut down already");
        } else {
            println!("Threadpool shutting down");
            // let mut b = *c;
            let sender = std::mem::replace(&mut *c, Box::new(None));
            let c =*sender;
            let b = c.unwrap();
            drop(b);
        }
    }

    /// Waits for the shutdown. 
    /// It is essentially waiting for the worker threads to complete
    /// This call blocks until all workers are complete
    pub fn wait_for_shutdown(self) {
        &self.init_shutdown();
        let w = self.workers;
        let mut d = w.lock();
        let a = d.as_mut().unwrap();
        let b =  &mut **a;
        while b.len() > 0 {
            let f = b.pop();
            if let Some(worker) = f {
                println!("Worker {} shutting down", worker.id);
                worker.shutdown();
            }
        }
        println!("Shutting down complete");
    }

    pub fn get_pending_tasks(&self) -> usize {
        self.pending_tasks.load(Ordering::SeqCst)
    }

    /// Returns the number of active workers.
    ///
    /// `Note:` This number might be less than `maximum_number_of_threads` because threads might have exited due errors
    pub fn number_active_workers(&self) -> usize {
        self.number_active_workers.load(Ordering::Relaxed)
    }

    /// Returns the number of tasks completed.
    ///
    pub fn number_of_tasks_completed(&self) -> u64 {
        todo!()
    }

    /// Executes the given Task some time in the future.
    /// ## Arguments
    /// * `task` - The Task to execute
    /// * `fail_fast` - if true returns immediately if no threads are available, if false enqueues the task.
    ///
    /// ## Examples
    /// ```ignore
    /// let result = thread_pool.execute(Task::new())
    ///                .expect("Unable to execute task")
    ///
    /// ```
    /// Returns the status of execution. At present it fails fast if no threads are available to perform.thread_pool
    /// TODO: add a method to accept the task and keep it in a queue.
    pub fn execute(&self, task: Task, fail_fast: bool) -> Result<(), ExecutionError> {
        timed!("ThreadPool.execute");
        let sender_reference = self.sender.borrow();
        if sender_reference.is_none() {
            return Err(ExecutionError::Shutdown);
        }
        if fail_fast && self.number_active_workers() == self.maximum_number_of_threads {
            return Err(ExecutionError::NoThreadAvailable);
        }
        self.pending_tasks.fetch_add(1, Ordering::SeqCst);
        let id = task.id.clone();
        // increment the count
        self.number_active_workers.fetch_add(1, Ordering::SeqCst);
        let sender = sender_reference.as_ref().as_ref().unwrap();
        sender.send(task).map_err(|_| {
            // decrement the value if we were not able to send it
            self.number_active_workers.fetch_sub(1, Ordering::SeqCst);
            ExecutionError::TaskChannelError
        })?;
        println!("Enqueued the task {}", id);
        self.check_and_recover_worker_deficit();
        Ok(())
    }

    /// Checks for deficit in the number of workers and adds new workers to cover the deficit
    fn check_and_recover_worker_deficit(&self) {
        timed!("ThreadPool.check_and_recover_worker_deficit");
        let mut workers = self.workers.lock().unwrap();
        let mut deficit = self.maximum_number_of_threads - workers.len();
        if deficit > 0 {
            println!(
                "There is a deficit of {} workers, will add new workers",
                deficit
            );
            let new_workers_count = deficit.clone();
            while deficit > 0 {
                let id = workers.len();
                workers.push(Worker::new(
                    id,
                    self.task_queue.clone(),
                    self.worker_callback.clone(),
                    self.number_active_workers.clone(),
                    self.pending_tasks.clone()
                ));
                deficit -= 1;
            }
            if new_workers_count > 0 {
                println!("Added {} new workers", new_workers_count);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionError, ThreadPool};
    use crate::task::Task;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    #[test]
    fn execute_runs_successfully() {
        let thread_pool = ThreadPool::new(5);
        let (test_sender, test_receiver) = mpsc::channel();
        let mut tasks: Vec<i32> = Vec::new();
        for i in 0..5 {
            let t = test_sender.clone();
            let task = Task::new(
                Box::new(move || {
                    t.send(i).unwrap();
                    thread::sleep(Duration::from_millis(100));
                }),
                i.to_string(),
            );
            thread_pool.execute(task, true).unwrap();
        }
        for _ in 0..5 {
            tasks.push(
                test_receiver
                    .recv_timeout(Duration::from_millis(100))
                    .unwrap(),
            );
        }
        tasks.sort();
        assert_eq!(vec![0, 1, 2, 3, 4], tasks);
    }

    #[test]
    fn fail_fast_execute_fails_if_max_threads_exceeded() {
        // Single thread
        let thread_pool = ThreadPool::new(1);
        let mut results: Vec<Result<(), ExecutionError>> = Vec::new();
        for i in 0..2 {
            let task = Task::new(
                Box::new(move || {
                    thread::sleep(Duration::from_millis(100));
                }),
                i.to_string(),
            );
            results.push(thread_pool.execute(task, true));
        }
        // Second thread should result in failure.
        assert_eq!(
            vec![Ok(()), Err(ExecutionError::NoThreadAvailable)],
            results
        );
    }

    #[test]
    fn execute_blocks_without_fail_fast() {
        let thread_pool = ThreadPool::new(1);
        let mut results: Vec<Result<(), ExecutionError>> = Vec::new();
        let mut outputs: Vec<i32> = Vec::new();
        let (tx, rx) = mpsc::channel();
        for i in 0..2 {
            let t = tx.clone();
            let task = Task::new(
                Box::new(move || {
                    thread::sleep(Duration::from_millis(100));
                    t.send(i).unwrap();
                }),
                i.to_string(),
            );
            results.push(thread_pool.execute(task, false));
        }
        loop {
            match rx.recv() {
                Ok(i) => {
                    outputs.push(i);
                }
                Err(_) => break,
            }
            // wait for two of those
            if outputs.len() == 2 {
                break;
            }
        }
        drop(rx);
        // Validate the thread execution results
        assert_eq!(vec![Ok(()), Ok(())], results);
        // Validate the sent results
        assert_eq!(vec![0, 1], outputs);
    }

    #[test]
    fn init_shutdown_stops_workers() {
        let thread_pool = ThreadPool::new(4);
        let mut results: Vec<Result<(), ExecutionError>> = Vec::new();
        let (tx, rx) = mpsc::channel();
        for i in 0..6 {
            let t = tx.clone();
            let task = Task::new(
                Box::new(move || {
                    thread::sleep(Duration::from_millis(100));
                    t.send(i).unwrap();
                }),
                i.to_string(),
            );
            results.push(thread_pool.execute(task, false));
            println!("Before shut down");
            if i == 4 {
                thread_pool.init_shutdown();
            }
        }

        let mut outputs= vec![
            rx.recv().ok(),
            rx.recv().ok(),
            rx.recv().ok(),
            rx.recv().ok()
        ];
        outputs.sort();
        drop(rx);
        // Validate the thread execution results
        assert_eq!(
            vec![
                Ok(()),
                Ok(()),
                Ok(()),
                Ok(()),
                Ok(()),
                Err(ExecutionError::Shutdown)
            ],
            results
        );
        // Validate the sent results
        assert_eq!(vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        ], outputs);
    }

    #[test]
    fn shutdown_stops_execution() {
        let thread_pool = ThreadPool::new(2);
        let mut results: Vec<Result<(), ExecutionError>> = Vec::new();
        let (tx, rx) = mpsc::channel();
        for i in 0..6 {
            let t = tx.clone();
            let task = Task::new(
                Box::new(move || {
                    thread::sleep(Duration::from_millis(100));
                    t.send(i).unwrap();
                }),
                i.to_string(),
            );
            results.push(thread_pool.execute(task, false));
        }
        thread_pool.wait_for_shutdown();

        let mut outputs= vec![
            rx.recv().ok(),
            rx.recv().ok(),
            rx.recv().ok(),
            rx.recv().ok()
        ];
        outputs.sort();
        drop(rx);
        // Validate the thread execution results
        assert_eq!(
            vec![
                Ok(()),
                Ok(()),
                Ok(()),
                Ok(()),
                Ok(()),
                Ok(())
            ],
            results
        );
        // Validate the sent results
        assert_eq!(vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        ], outputs);
    }
}
