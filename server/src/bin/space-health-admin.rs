use space_health_server::admin::{self, AdminError};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Result<Vec<_>, _> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.into_string())
        .collect();
    let result = match args {
        Ok(args) => admin::execute(&args).await,
        Err(_) => Err(AdminError::Usage),
    };
    match result {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"error": {"code": error.code(), "message": error.to_string()}})
            );
            ExitCode::from(error.exit_code())
        }
    }
}
