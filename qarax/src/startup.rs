use std::future::IntoFuture;
use tokio::net::TcpListener;

use sqlx::PgPool;

use crate::{App, configuration::VmDefaultsSettings, handlers::app};

pub async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    vm_defaults: VmDefaultsSettings,
) -> Result<impl IntoFuture<Output = std::io::Result<()>> + Send, Box<dyn std::error::Error + Send>>
{
    let a = App::new(db_pool, vm_defaults);

    // Spawn background task to reconcile VM status with the live node state
    tokio::spawn(crate::vm_monitor::start_vm_monitor(a.pool_arc()));

    let app = app(a);
    let server = axum::serve(listener, app);
    Ok(server)
}
