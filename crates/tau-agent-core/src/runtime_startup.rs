//! Startup/runtime infrastructure helpers (cancellation, async events, and local caches).

use std::{
    collections::{HashMap, VecDeque},
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

use crate::{
    AgentDirectMessageError, AgentError, AgentEvent, AsyncEventDispatchMetricsInner,
    AsyncEventHandler, CooperativeCancellationToken,
};

pub(crate) async fn sleep_with_cancellation(
    delay: Duration,
    cancellation_token: Option<CooperativeCancellationToken>,
) -> Result<(), AgentError> {
    if let Some(token) = cancellation_token {
        tokio::select! {
            _ = token.cancelled() => Err(AgentError::Cancelled),
            _ = tokio::time::sleep(delay) => Ok(()),
        }
    } else {
        tokio::time::sleep(delay).await;
        Ok(())
    }
}

pub(crate) fn spawn_async_event_handler_worker(
    receiver: std::sync::mpsc::Receiver<AgentEvent>,
    handler: AsyncEventHandler,
    timeout: Option<Duration>,
    metrics: Arc<AsyncEventDispatchMetricsInner>,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };

        while let Ok(event) = receiver.recv() {
            let handler = Arc::clone(&handler);
            let metrics = Arc::clone(&metrics);
            runtime.block_on(async move {
                let mut task = tokio::spawn(async move { (handler)(event).await });
                if let Some(timeout) = timeout {
                    match tokio::time::timeout(timeout, &mut task).await {
                        Ok(Ok(())) => {
                            metrics.completed.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(Err(_)) => {
                            metrics.panicked.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            task.abort();
                            let _ = task.await;
                            metrics.timed_out.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                } else {
                    match task.await {
                        Ok(()) => {
                            metrics.completed.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            metrics.panicked.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
        }
    });
}

pub(crate) fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(crate) fn cache_insert_with_limit<T: Clone>(
    cache: &mut HashMap<String, T>,
    order: &mut VecDeque<String>,
    key: String,
    value: T,
    max_entries: usize,
) {
    if max_entries == 0 {
        return;
    }
    if let Some(position) = order.iter().position(|entry| entry == &key) {
        order.remove(position);
    }
    order.push_back(key.clone());
    cache.insert(key, value);

    while cache.len() > max_entries {
        let Some(oldest) = order.pop_front() else {
            break;
        };
        cache.remove(&oldest);
    }
}
pub(crate) fn normalize_direct_message_content(
    content: &str,
    max_message_chars: usize,
) -> Result<String, AgentDirectMessageError> {
    let normalized = content.trim();
    if normalized.is_empty() {
        return Err(AgentDirectMessageError::EmptyContent);
    }
    let actual_chars = normalized.chars().count();
    if actual_chars > max_message_chars {
        return Err(AgentDirectMessageError::MessageTooLong {
            actual_chars,
            max_chars: max_message_chars,
        });
    }
    Ok(normalized.to_string())
}
