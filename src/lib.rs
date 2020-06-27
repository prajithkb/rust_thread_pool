//! A simple thread pool (only fixed size support now) with a fail fast logic to return if there are no threds available to execute.
//! ```ignore
//! use thread_pool::ThreadPool;
//! // Initiatize with 1 threads
//! let mut thread_pool = ThreadPool::new(1);
//! let first_task_execution = thread_pool.execute(Box::new(move || {
//!     thread::sleep(Duration::from_millis(100));
//! }));
//! // Success
//! assert_eq!(Ok(()), first_task_execution)
//! let second_task_execution = thread_pool.execute(Box::new(move || {
//!     thread::sleep(Duration::from_millis(100));
//! }));
//! // fails since there are no threads available to execute this task
//! assert_eq!(Err((ExecutionError::NoThreadAvailable)), second_task_execution)
//!
//! // Shutting down
//! thread_pool.await_shutdown(); // This will block until all the workers are closed.
//! ```
//!
extern crate timed;

/// Used to guard println statements, this lets you add as many print statements as needed and provides 
/// an option to opt out of that
#[macro_use]
mod print_macro {
    macro_rules! guarded_println {
        () => (if let Err(_) = std::env::var("PRINT_DISABLED") {
            println!();
        } else {
           
        });
        ($($arg:tt)*) => ({
            if let Err(_) = std::env::var("PRINT_DISABLED") {
                println!($($arg)*);
            } else {
                
            }
        })
    }
}
pub mod task;
pub mod thread_pool;
mod worker;

#[cfg(test)]
mod tests {
    use crate::task::Task;
    use crate::thread_pool::ThreadPool;
    use std::sync::mpsc::Sender;
    use std::thread;
    use std::time::Duration;
    use std::{fs, sync::mpsc};

    #[test]
    fn main_test() {
        run_tests(100);
    }

    fn run_tests(number_of_runs: usize) {
        let (tx, rx) = mpsc::channel();
        let thread_pool = ThreadPool::new(10);
        for i in 0..number_of_runs {
            thread_pool
                .execute(task(i.to_string(), tx.clone()), false)
                .unwrap();
        }
        let mut u = thread_pool.get_pending_tasks();
        while u > 0 {
            println!("PENDING TASKS {}", u);
            thread::sleep(Duration::from_millis(100));
            u = thread_pool.get_pending_tasks();
        }
        thread_pool.wait_for_shutdown();
        let mut number_of_messages = 0;
        drop(tx);
        loop {
            match rx.recv() {
                Ok(_) => number_of_messages += 1,
                Err(_) => break,
            }
        }
        let mut expected = 0;
        let paths = fs::read_dir("./").unwrap();
        for _ in paths {
            expected += 1;
        }
        assert_eq!(number_of_runs * expected, number_of_messages);
    }
    fn task(id: String, tx: Sender<String>) -> Task {
        Task::new(
            Box::new(move || {
                let paths = fs::read_dir("./").unwrap();
                for path in paths {
                    let a = path.unwrap().path();
                    let p = a.display();
                    tx.send(p.to_string()).unwrap();
                }
            }),
            id,
        )
    }
}
