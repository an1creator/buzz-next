//! Bounded, owner-scoped cancellation for explicit connection checks.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use tokio::sync::watch;
type Operations = HashMap<(String, String), watch::Sender<bool>>;
fn operations() -> &'static Mutex<Operations> {
    static OPERATIONS: OnceLock<Mutex<Operations>> = OnceLock::new();
    OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) struct Operation {
    key: (String, String),
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut active) = operations().lock() {
            active.remove(&self.key);
        }
    }
}
pub(crate) fn begin(owner: &str, id: &str) -> Result<(Operation, watch::Receiver<bool>), String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid check ID")?;
    let key = (owner.to_string(), id.to_string());
    let mut active = operations()
        .lock()
        .map_err(|_| "Connection checks unavailable")?;
    if active.len() >= 4 {
        return Err("Other connection checks are running. Try again shortly.".into());
    }
    if active.contains_key(&key) {
        return Err("This check is already running".into());
    }
    let (sender, receiver) = watch::channel(false);
    active.insert(key.clone(), sender);
    Ok((Operation { key }, receiver))
}
pub(crate) fn cancel(owner: &str, id: &str) -> Result<(), String> {
    let active = operations()
        .lock()
        .map_err(|_| "Connection checks unavailable")?;
    if let Some(sender) = active.get(&(owner.to_string(), id.to_string())) {
        sender.send_replace(true);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_is_owned_and_cleanup_allows_retry() {
        let id = uuid::Uuid::new_v4().to_string();
        let (guard, mut signal) = begin("owner", &id).unwrap();
        assert!(begin("owner", &id).is_err());
        cancel("other", &id).unwrap();
        assert!(!*signal.borrow());
        cancel("owner", &id).unwrap();
        signal.changed().await.unwrap();
        assert!(*signal.borrow());
        drop(guard);
        assert!(begin("owner", &id).is_ok());
    }
}
