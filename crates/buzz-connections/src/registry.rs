//! Pure transitions used inside the Desktop's atomic store transaction.
use crate::model::{Connection, ConnectionCheck, Target};
use serde::{Deserialize, Serialize};

/// Empty by default; merely opening Settings never creates a connection.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub connections: Vec<Connection>,
}

impl Registry {
    /// Save with optimistic concurrency. A caller cannot forge a readiness result.
    pub fn save(
        &mut self,
        mut value: Connection,
        expected_revision: Option<u64>,
    ) -> Result<Connection, String> {
        value.validate()?;
        let index = self.connections.iter().position(|item| item.id == value.id);
        if matches!(value.target, Target::Local)
            && self
                .connections
                .iter()
                .any(|item| item.id != value.id && matches!(item.target, Target::Local))
        {
            return Err("This device is already added. Open the existing connection.".into());
        }
        match index {
            Some(index) => {
                let old = &self.connections[index];
                if expected_revision != Some(old.revision) {
                    return Err("Connection changed. Reload and try again.".into());
                }
                if std::mem::discriminant(&old.target) != std::mem::discriminant(&value.target) {
                    return Err("Connection type cannot change. Add a new connection.".into());
                }
                value.revision = old
                    .revision
                    .checked_add(1)
                    .ok_or("Connection revision exhausted")?;
                value.check = if value.target == old.target {
                    old.check.clone().map(|mut check| {
                        check.revision = value.revision;
                        check
                    })
                } else {
                    None
                };
                self.connections[index] = value.clone();
            }
            None => {
                if expected_revision.is_some() {
                    return Err("Connection no longer exists".into());
                }
                if self.connections.len() >= 128 {
                    return Err("Connection limit reached".into());
                }
                value.revision = 1;
                value.check = None;
                self.connections.push(value.clone());
            }
        }
        Ok(value)
    }

    /// Attach an observation only if its input revision remains current.
    pub fn record_check(&mut self, id: &str, check: ConnectionCheck) -> Result<(), String> {
        let item = self
            .connections
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("Connection no longer exists")?;
        if item.revision != check.revision {
            return Err("Connection changed during the check".into());
        }
        item.check = Some(check);
        Ok(())
    }

    /// The caller computes references under the same store lock as this transition.
    pub fn remove(
        &mut self,
        id: &str,
        expected_revision: u64,
        referenced_by: &[String],
    ) -> Result<(), String> {
        let item = self
            .connections
            .iter()
            .find(|item| item.id == id)
            .ok_or("Connection no longer exists")?;
        if item.revision != expected_revision {
            return Err("Connection changed. Reload and try again.".into());
        }
        if !referenced_by.is_empty() {
            return Err(
                "This connection is used by agents. Reassign stopped agents before removing it."
                    .into(),
            );
        }
        self.connections.retain(|item| item.id != id);
        Ok(())
    }
}
