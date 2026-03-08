use rapina::prelude::*;
use rapina::schemars;

#[public]
#[get("/health")]
pub async fn health() -> Result<Json<HealthResponse>> {
    Ok(Json(HealthResponse { status: "ok" }))
}

#[derive(Serialize, JsonSchema)]
pub struct HealthResponse {
    status: &'static str,
}
