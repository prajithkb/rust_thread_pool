use crate::task::Task;
use crate::timed_execution::log_time;
use crate::{
    timed,
    worker::{Worker, WorkerCallback},
};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use mpsc::{Receiver, Sender};
use std::{
    sync::Arc,
    sync::{mpsc, Mutex},
};

/// Defines the error states returned by `thread_pool.execute`
#[derive(Debug, PartialEq)]
pub enum ExecutionError {
    /// No threads are available to perform this task
    NoThreadAvailable,
    /// The internal channel for submitting the tasks failed
    TaskChannelError
}

/// A runnable function, declared as a type alias
pub type Runnable = Box<dyn FnOnce() + Send + 'static>;
/// A simple fixed size Thread pool, that has a manages a set of 'workers' to execute [`Runnable`][Runnable] types
///
/// [Runnable]: ./ThreadPool.html#type.Runnable  
pub struct ThreadPool {
    workers: Arc<Mutex<Vec<Worker>>>,
    sender: Sender<Task>,
    number_active_workers: Arc<AtomicUsize>,
    maximum_number_of_threads: usize,
    task_queue: Arc<Mutex<Receiver<Task>>>,
    worker_callback: WorkerCallback,
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
        let (sender, r): (Sender<Task>, Receiver<Task>) = mpsc::channel();
        let task_queue: Arc<Mutex<Receiver<Task>>> = Arc::new(Mutex::new(r));
        let workers: Arc<Mutex<Vec<Worker>>> =
            Arc::new(Mutex::new(Vec::with_capacity(maximum_number_of_threads)));
        let workers_for_callback = workers.clone();
        let w: WorkerCallback = Arc::new(Mutex::new(move |id, result: Result<(), String>| {
            println!("Removing worker due to {}", result.unwrap_err());
            let mut w = workers_for_callback.lock().unwrap();
            w.remove(id);
            println!("Number of workers {}", w.len());
        }));
        for id in 0..maximum_number_of_threads {
            let remove_item: WorkerCallback = w.clone();
            let worker = Worker::new(
                id,
                task_queue.clone(),
                remove_item,
                number_active_workers.clone(),
            );
            workers.lock().unwrap().push(worker);
        }
        let thread_pool = ThreadPool {
            workers,
            sender,
            number_active_workers,
            maximum_number_of_threads,
            task_queue,
            worker_callback: w,
        };
        thread_pool
    }

    /// Shuts down the Thread pool
    /// All the Workers are interrupted
    pub fn shutdown() {
        todo!()
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
    ///
    /// ## Examples
    /// ```ignore
    /// let result = thread_pool.execute(Task::new())
    ///                .expect("Unable to execute task")
    ///
    /// ```
    /// Returns the status of execution. At present it fails fast if no threads are available to perform.thread_pool
    /// TODO: add a method to accept the task and keep it in a queue.
    pub fn execute(&mut self, task: Task) -> Result<(), ExecutionError> {
        timed!("ThreadPool.execute");
        if self.number_active_workers() == self.maximum_number_of_threads {
            return Err(ExecutionError::NoThreadAvailable);
        }
        let id = task.id.clone();
        // increment the count
        self.number_active_workers.fetch_add(1, Ordering::SeqCst);
        self.sender.send(task).map_err(|_| {
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
    use super::{ThreadPool, ExecutionError};
    use crate::task::Task;
    use std::time::Duration;
    use std::sync::{mpsc};
    use std::thread;
    #[test]
    fn execute_runs_successfully() {
        let mut thread_pool = ThreadPool::new(5);
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
            thread_pool.execute(task).unwrap();
        }
        for _ in 0..5 {
            tasks.push(
                test_receiver
                    .recv_timeout(Duration::from_millis(100)).unwrap(),
            );
        }
        tasks.sort();
        assert_eq!(vec![0, 1, 2, 3, 4], tasks);
    }

    #[test]
    fn execute_fails_if_max_threads_exceeded() {
        // Single thread
        let mut thread_pool = ThreadPool::new(1);
        let mut results:  Vec<Result<(),ExecutionError>>= Vec::new();
        for i in 0..2 {
            let task = Task::new(
                Box::new(move || {
                    thread::sleep(Duration::from_millis(100));
                }),
                i.to_string(),
            );
            results.push(thread_pool.execute(task));
        }
        // Second thread should result in failure.
        assert_eq!(vec![Ok(()), Err(ExecutionError::NoThreadAvailable)], results);
    }
}
