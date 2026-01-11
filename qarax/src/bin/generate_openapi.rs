use std::fs;
use utoipa::OpenApi;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generate the OpenAPI spec
    let openapi = qarax::handlers::ApiDoc::openapi();

    // Convert to YAML
    let yaml = serde_yaml::to_string(&openapi)?;

    // Write to file in the qarax directory
    let output_path = "openapi.yaml";
    fs::write(output_path, yaml)?;

    println!("OpenAPI spec written to {}", output_path);

    Ok(())
}
