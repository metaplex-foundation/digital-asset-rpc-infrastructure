use std::{fmt::Display, future::Future, time::Duration};

use log::warn;
use plerkle_messenger::{Messenger, MessengerConfig};
use tokio::{
    task::{AbortHandle, JoinHandle, JoinSet},
    time::{sleep, timeout_at, Instant},
};

use crate::error::IngesterError;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

/// Docker may restore this process before Redis is accepting connections.
/// Retry initialization; an exhausted deadline must reach the supervisor.
pub async fn connect_messenger<T: Messenger>(
    config: MessengerConfig,
    worker: &str,
) -> Result<T, IngesterError> {
    retry_connection(worker, STARTUP_TIMEOUT, || T::new(config.clone())).await
}

async fn retry_connection<T, E, F, Fut>(
    worker: &str,
    startup_timeout: Duration,
    mut connect: F,
) -> Result<T, IngesterError>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let deadline = Instant::now() + startup_timeout;
    let mut delay = INITIAL_RETRY_DELAY;
    loop {
        let failure = match timeout_at(deadline, connect()).await {
            Ok(Ok(messenger)) => return Ok(messenger),
            Ok(Err(error)) => error.to_string(),
            Err(_) => "connection attempt exceeded the startup deadline".to_string(),
        };
        if Instant::now() >= deadline {
            return Err(IngesterError::UnrecoverableTaskError(format!(
                "{worker} could not connect to Redis within {startup_timeout:?}: {failure}"
            )));
        }
        warn!("{worker} Redis initialization failed; retrying: {failure}");
        sleep(delay.min(deadline.saturating_duration_since(Instant::now()))).await;
        delay = delay.saturating_mul(2).min(MAX_RETRY_DELAY);
    }
}

struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Required workers must run for the lifetime of the process. Preserve their
/// errors/panics and abort the underlying task when its supervisor shuts down.
pub fn supervise_worker(
    workers: &mut JoinSet<IngesterError>,
    name: &'static str,
    task: JoinHandle<Result<(), IngesterError>>,
) {
    let guard = AbortOnDrop(task.abort_handle());
    workers.spawn(async move {
        let _guard = guard;
        let reason = match task.await {
            Ok(Ok(())) => "exited unexpectedly".to_string(),
            Ok(Err(error)) => error.to_string(),
            Err(error) => error.to_string(),
        };
        IngesterError::UnrecoverableTaskError(format!("{name}: {reason}"))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::pending, sync::Arc};
    use tokio::sync::Notify;

    #[tokio::test(start_paused = true)]
    async fn redis_becoming_ready_after_startup_is_retried() {
        let mut attempts = 0;
        let connected = retry_connection("account", Duration::from_secs(60), || {
            attempts += 1;
            std::future::ready(if attempts < 4 {
                Err("connection refused")
            } else {
                Ok(42)
            })
        })
        .await
        .unwrap();
        assert_eq!(connected, 42);
        assert_eq!(attempts, 4);
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_redis_exhausts_a_bounded_deadline() {
        let began = Instant::now();
        let result = retry_connection("account", Duration::from_secs(2), || async {
            Err::<(), _>("connection refused")
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("account"));
        assert_eq!(began.elapsed(), Duration::from_secs(2));
    }

    #[tokio::test(start_paused = true)]
    async fn a_hung_connection_attempt_cannot_bypass_the_deadline() {
        let began = Instant::now();
        let result = retry_connection("ack", Duration::from_secs(2), || {
            pending::<Result<(), &str>>()
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("deadline"));
        assert_eq!(began.elapsed(), Duration::from_secs(2));
    }

    #[tokio::test]
    async fn consumer_completion_failure_and_panic_are_all_fatal() {
        for outcome in 0..3 {
            let mut workers = JoinSet::new();
            supervise_worker(
                &mut workers,
                "account",
                tokio::spawn(async move {
                    match outcome {
                        0 => Ok(()),
                        1 => Err(IngesterError::UnrecoverableTaskError(
                            "Redis unavailable".into(),
                        )),
                        _ => panic!("consumer panic"),
                    }
                }),
            );
            let error = workers.join_next().await.unwrap().unwrap();
            assert!(error.to_string().contains("account"));
        }
    }

    #[tokio::test]
    async fn stopping_supervision_aborts_the_underlying_consumer() {
        let started = Arc::new(Notify::new());
        let signal = Arc::clone(&started);
        let task = tokio::spawn(async move {
            signal.notify_one();
            pending::<Result<(), IngesterError>>().await
        });
        let abort = task.abort_handle();
        let mut workers = JoinSet::new();
        supervise_worker(&mut workers, "account", task);
        started.notified().await;
        workers.shutdown().await;
        tokio::task::yield_now().await;
        assert!(abort.is_finished());
    }
}
