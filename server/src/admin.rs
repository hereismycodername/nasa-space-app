//! Local administrative commands. Device registration is intentionally not an HTTP API.
use crate::{database, domain::history::DEMO_ASTRONAUT_ID, storage::devices};
use serde_json::{json, Value};

/// A stable, machine-readable admin command failure.
#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    /// The command name or number of arguments is invalid.
    #[error("Usage: space-health-admin register-device <id> | list-devices | --help")]
    Usage,
    /// Device identifiers must match measurement ingestion's identifier bounds.
    #[error("Device ID must contain from 1 to 128 UTF-8 bytes")]
    InvalidDeviceId,
    /// An existing registration cannot be reassigned by this command.
    #[error("Device ID already belongs to a different source or astronaut; choose another ID")]
    DeviceConflict,
    /// The shared database configuration or migrations failed.
    #[error("Could not open or migrate the database; check DATABASE_URL and file permissions")]
    Database(#[from] database::DatabaseError),
    /// A device read or registration failed without an acknowledged write.
    #[error("Device database operation failed")]
    Storage(#[from] sqlx::Error),
}

impl AdminError {
    /// Stable error code for scripts; human-readable messages may evolve.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Usage => "INVALID_ARGUMENTS",
            Self::InvalidDeviceId => "INVALID_DEVICE_ID",
            Self::DeviceConflict => "DEVICE_CONFLICT",
            Self::Database(_) | Self::Storage(_) => "DATABASE_ERROR",
        }
    }

    /// Exit status: 2 for invalid input, 1 for conflicts or database failures.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage | Self::InvalidDeviceId => 2,
            _ => 1,
        }
    }
}

fn device_json(device: devices::DeviceRecord) -> Value {
    json!({
        "deviceId": device.id,
        "astronautId": device.astronaut_id,
        "source": device.source,
    })
}

/// Execute one command using DATABASE_URL and return its JSON result.
///
/// `register-device <id>` always assigns the server's demo astronaut and `sensor`
/// source. An identical registration succeeds with `created: false`; conflicting
/// existing ownership or source is never overwritten. `list-devices` returns all
/// registrations sorted by ID. Invalid arguments do not open or create a database.
pub async fn execute(args: &[String]) -> Result<Value, AdminError> {
    let command = args.first().map(String::as_str);
    match (command, args.len()) {
        (Some("--help" | "help"), 1) => {
            return Ok(json!({
                "commands": ["register-device <id>", "list-devices"],
                "databaseEnvironment": "DATABASE_URL",
                "defaultDatabaseUrl": database::DEFAULT_DATABASE_URL,
            }))
        }
        (Some("register-device"), 2) => {
            if args[1].is_empty() || args[1].len() > 128 {
                return Err(AdminError::InvalidDeviceId);
            }
        }
        (Some("list-devices"), 1) => {}
        _ => return Err(AdminError::Usage),
    }
    let pool = database::connect_from_env().await?;
    let result = if command == Some("register-device") {
        let registered = devices::register_sensor(&pool, &args[1], DEMO_ASTRONAUT_ID).await?;
        if registered.device.astronaut_id != DEMO_ASTRONAUT_ID
            || registered.device.source != "sensor"
        {
            Err(AdminError::DeviceConflict)
        } else {
            Ok(json!({"created": registered.created, "device": device_json(registered.device)}))
        }
    } else {
        let devices = devices::list(&pool)
            .await?
            .into_iter()
            .map(device_json)
            .collect::<Vec<_>>();
        Ok(json!({"devices": devices}))
    };
    pool.close().await;
    result
}
