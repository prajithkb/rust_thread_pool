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
//! ```
//! 
#[macro_use]
extern crate lazy_static;
pub mod task;
pub mod thread_pool;
mod timed_execution;
mod worker;
