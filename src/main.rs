use rapina::prelude::*;
use rapina::middleware::RequestLogMiddleware;
use rapina::schemars;

#[derive(Serialize, JsonSchema)]
struct MessageResponse {
    message: String,
}

#[derive(Serialize, JsonSchema)]
struct HealthResponse {
    status: String,
    version: String,
}

#[get("/")]
async fn hello() -> Json<MessageResponse> {
    Json(MessageResponse {
        message: "Hello from Rapina!".to_string(),
    })
}

#[get("/health")]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let router = Router::new()
        .get("/", hello)
        .get("/health", health);

    Rapina::new()
        .with_tracing(TracingConfig::new())
        .middleware(RequestLogMiddleware::new())
        .router(router)
        .listen("127.0.0.1:3000")
        .await
}
