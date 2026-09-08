use mentat_core::MentatError;
use mentat_storage::SqliteStorage;
use tokio::sync::mpsc;
use uuid::Uuid;

/// UI 밖에서 250ms/4KiB 기준으로 누적하고 채널 종료 전에 마지막 batch를 저장한다.
pub async fn persist_deltas(
    storage: Option<SqliteStorage>,
    message: Uuid,
    mut rx: mpsc::UnboundedReceiver<String>,
) -> Result<(), MentatError> {
    let mut buffer = String::new();
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        let closed = tokio::select! {
            delta = rx.recv() => match delta { Some(delta) => { buffer.push_str(&delta); false }, None => true },
            _ = timer.tick() => {
                flush(&storage, message, &mut buffer).await?;
                false
            }
        };
        if closed || buffer.len() >= 4096 {
            flush(&storage, message, &mut buffer).await?;
        }
        if closed {
            return Ok(());
        }
    }
}
async fn flush(
    storage: &Option<SqliteStorage>,
    id: Uuid,
    buffer: &mut String,
) -> Result<(), MentatError> {
    if buffer.is_empty() {
        return Ok(());
    }
    let delta = std::mem::take(buffer);
    if let Some(storage) = storage.clone() {
        tokio::task::spawn_blocking(move || storage.append_assistant_delta(id, &delta))
            .await
            .map_err(|e| MentatError::IoError(e.to_string()))??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn thousand_deltas_are_drained_before_worker_completion() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        for _ in 0..1000 {
            tx.send("한글 delta\n".to_string()).unwrap();
        }
        drop(tx);
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            super::persist_deltas(None, uuid::Uuid::new_v4(), rx),
        )
        .await
        .unwrap()
        .unwrap();
    }
}
