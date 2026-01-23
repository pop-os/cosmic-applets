use futures_util::future::select;

/// Spawn a background tasks and forward its messages
pub fn forward_event_loop<M: 'static + Send, T: Future<Output = ()> + Send + 'static>(
    event_loop: impl FnOnce(async_fn_stream::StreamEmitter<M>) -> T + Send + 'static,
) -> (tokio::sync::oneshot::Sender<()>, cosmic::Task<M>) {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let task = cosmic::Task::stream(async_fn_stream::fn_stream(|emitter| async move {
        select(
            std::pin::pin!(cancel_rx),
            std::pin::pin!(event_loop(emitter)),
        )
        .await;
    }));

    (cancel_tx, task)
}
