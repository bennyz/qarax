use axum::{Router, routing::IntoMakeService, serve::Serve};
use tokio::net::TcpListener;

use sqlx::PgPool;

use crate::{App, configuration::VmDefaultsSettings, handlers::app};

pub async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    qarax_node_address: String,
    vm_defaults: VmDefaultsSettings,
) -> Result<Serve<IntoMakeService<Router>, Router>, Box<dyn std::error::Error + Send>> {
    let a = App::new(db_pool, qarax_node_address, vm_defaults);

    // Spawn background task to reconcile VM status with the live node state
    tokio::spawn(crate::vm_monitor::start_vm_monitor(
        a.pool_arc(),
        a.qarax_node_address().to_string(),
    ));

    let app = app(a);
    let server = axum::serve(listener, app.into_make_service());
    Ok(server)
}
