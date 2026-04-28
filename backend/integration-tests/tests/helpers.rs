use std::net::SocketAddr;
use std::sync::Arc;

use api_server::router;
use api_server::state::AppState;
use shared::config::Config;
use shared::storage::LocalStorage;
use sqlx::PgPool;

pub struct TestApp {
    pub addr: SocketAddr,
    pub _client: reqwest::Client,
}

pub async fn spawn_app(pool: PgPool) -> TestApp {
    let config = Config::from_env().expect("failed to load config");

    let storage = Arc::new(LocalStorage::new(&config.storage_root));

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        storage,
    };

    let app = router::router().with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test listener");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Wait a moment for the server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();

    TestApp { addr, _client: client }
}

pub fn api_url(app: &TestApp, path: &str) -> String {
    format!("http://{}{}", app.addr, path)
}
