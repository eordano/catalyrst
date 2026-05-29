use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::debug;

#[async_trait::async_trait]
pub trait ISequentialTaskExecutor: Send + Sync {
    async fn run<T: Send + 'static>(
        &self,
        job_name: &str,
        task: Box<dyn FnOnce() -> std::pin::Pin<Box<dyn Future<Output = T> + Send>> + Send>,
    ) -> T;
}

pub struct SequentialTaskExecutor {
    queues: parking_lot::Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl SequentialTaskExecutor {
    pub fn new() -> Self {
        Self {
            queues: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    fn get_queue(&self, job_name: &str) -> Arc<Mutex<()>> {
        let mut queues = self.queues.lock();
        queues
            .entry(job_name.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn run_fn<T, F, Fut>(&self, job_name: &str, f: F) -> T
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = T> + Send,
        T: Send,
    {
        let queue = self.get_queue(job_name);
        let _guard = queue.lock().await;

        debug!(job_name, "sequential task started");

        let result = f().await;

        debug!(job_name, "sequential task finished");

        result
    }
}

impl Default for SequentialTaskExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn tasks_on_same_queue_run_sequentially() {
        let executor = SequentialTaskExecutor::new();
        let counter = Arc::new(AtomicU32::new(0));

        let c1 = counter.clone();
        let c2 = counter.clone();

        let handle1 = {
            let executor = &executor;
            tokio::spawn({
                let c = c1;
                let q = executor.get_queue("test");
                async move {
                    let _g = q.lock().await;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    c.fetch_add(1, Ordering::SeqCst);
                    c.load(Ordering::SeqCst)
                }
            })
        };

        tokio::time::sleep(Duration::from_millis(5)).await;

        let handle2 = {
            let executor = &executor;
            tokio::spawn({
                let c = c2;
                let q = executor.get_queue("test");
                async move {
                    let _g = q.lock().await;
                    c.fetch_add(1, Ordering::SeqCst);
                    c.load(Ordering::SeqCst)
                }
            })
        };

        let r1 = handle1.await.unwrap();
        let r2 = handle2.await.unwrap();

        assert_eq!(r1, 1);
        assert_eq!(r2, 2);
    }

    #[tokio::test]
    async fn tasks_on_different_queues_run_concurrently() {
        let executor = SequentialTaskExecutor::new();
        let counter = Arc::new(AtomicU32::new(0));

        let c1 = counter.clone();
        let c2 = counter.clone();

        let q_a = executor.get_queue("queue-a");
        let q_b = executor.get_queue("queue-b");

        let handle1 = tokio::spawn(async move {
            let _g = q_a.lock().await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            c1.fetch_add(1, Ordering::SeqCst)
        });

        let handle2 = tokio::spawn(async move {
            let _g = q_b.lock().await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            c2.fetch_add(1, Ordering::SeqCst)
        });

        handle1.await.unwrap();
        handle2.await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn run_fn_helper() {
        let executor = SequentialTaskExecutor::new();
        let result = executor
            .run_fn("my-job", || async { 42 })
            .await;
        assert_eq!(result, 42);
    }
}
