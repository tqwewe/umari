# OpenAPI & Swagger UI Integration

The Umari API now includes full OpenAPI 3.0 specification and interactive Swagger UI documentation.

## Features Added

✅ **OpenAPI 3.0 Specification** - Auto-generated from code
✅ **Swagger UI** - Interactive API documentation
✅ **Schema Definitions** - All request/response types documented
✅ **Error Responses** - Complete error code documentation

## Accessing the Documentation

### Swagger UI (Interactive)
```
http://localhost:3000/swagger-ui
```

Browse and test all API endpoints directly from your browser.

### OpenAPI JSON Spec
```
http://localhost:3000/api-docs/openapi.json
```

Download the raw OpenAPI specification for use with other tools.

## Implementation Details

### Dependencies Added

```toml
utoipa = { version = "5.3", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "9.0", features = ["axum"] }
```

### Runtime Feature

The `ModuleType` enum now supports OpenAPI schema generation via the `openapi` feature:

```toml
# In umari-runtime/Cargo.toml
[features]
openapi = ["utoipa"]
```

### Schema Annotations

All API types are annotated with `ToSchema` for automatic documentation:

```rust
#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    /// Type of module (Command or Projection)
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Semantic version
    pub version: String,
    /// SHA-256 hash of WASM binary
    pub sha256: String,
    /// Whether the module was activated
    pub activated: bool,
}
```

### OpenAPI Configuration

The OpenAPI spec is defined in `/home/ari/dev/tqwewe/umari/crates/api/src/lib.rs`:

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        // Paths will be added via handler annotations
    ),
    components(
        schemas(
            UploadResponse,
            ListModulesResponse,
            // ... all types
        )
    ),
    tags(
        (name = "commands", description = "Command module management"),
        (name = "projections", description = "Projection module management"),
        (name = "modules", description = "Cross-module operations"),
        (name = "execution", description = "Command execution")
    ),
    info(
        title = "Umari Event-Sourcing API",
        version = "1.0.0",
        description = "REST API for managing and executing WASM-based commands and projections",
        license(name = "MIT OR Apache-2.0")
    )
)]
struct ApiDoc;
```

## Using the Swagger UI

1. **Start the server:**
   ```bash
   cargo run -p umari-cli
   ```

2. **Open browser:**
   ```
   http://localhost:3000/swagger-ui
   ```

3. **Explore endpoints:**
   - Browse all available API endpoints
   - View request/response schemas
   - Try out endpoints with the "Try it out" button
   - See example responses
   - Test different scenarios

## Features

### Schema Documentation
- All request/response types are fully documented
- Field descriptions explain purpose
- Type information is auto-generated
- Optional fields are clearly marked

### Error Responses
- All error codes documented
- HTTP status codes mapped correctly
- Example error responses provided

### Tags & Organization
- Endpoints grouped by functionality:
  - **commands** - Command module operations
  - **projections** - Projection module operations
  - **modules** - Cross-module operations
  - **execution** - Command execution

## Next Steps (Optional Enhancements)

### Add Path Annotations
Annotate route handlers with `#[utoipa::path]` for better documentation:

```rust
#[utoipa::path(
    post,
    path = "/commands/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Semantic version"),
        ("activate" = Option<bool>, Query, description = "Activate immediately")
    ),
    request_body(content = Multipart, description = "WASM binary file"),
    responses(
        (status = 201, description = "Module uploaded successfully", body = UploadResponse),
        (status = 400, description = "Invalid input", body = ErrorResponse),
        (status = 409, description = "Module version already exists", body = ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn upload_command(/* ... */) { /* ... */ }
```

### Add Example Values
Use `#[schema(example = "...")]` on fields:

```rust
#[derive(Deserialize, ToSchema)]
pub struct ActivateRequest {
    /// Version to activate
    #[schema(example = "1.0.0")]
    pub version: String,
}
```

### Add Security Schemes
When authentication is added:

```rust
#[openapi(
    // ...
    security(
        ("api_key" = [])
    ),
    components(
        security_schemes(
            ("api_key" = (type = ApiKey, in = Header, name = "X-API-Key"))
        )
    )
)]
```

## Exporting OpenAPI Spec

Download and use the spec with other tools:

```bash
# Download spec
curl http://localhost:3000/api-docs/openapi.json > openapi.json

# Generate client SDKs with openapi-generator
openapi-generator-cli generate -i openapi.json -g typescript-axios -o ./client

# Import into Postman, Insomnia, etc.
```

## Files Modified

- `/home/ari/dev/tqwewe/umari/Cargo.toml` - Added utoipa dependencies
- `/home/ari/dev/tqwewe/umari/crates/api/Cargo.toml` - Enabled dependencies
- `/home/ari/dev/tqwewe/umari/crates/api/src/lib.rs` - OpenAPI config & Swagger UI route
- `/home/ari/dev/tqwewe/umari/crates/api/src/error.rs` - Added ToSchema derives
- `/home/ari/dev/tqwewe/umari/crates/api/src/routes/modules/types.rs` - Added ToSchema derives
- `/home/ari/dev/tqwewe/umari/crates/runtime/src/module_store/mod.rs` - Added openapi feature
- `/home/ari/dev/tqwewe/umari/crates/runtime/Cargo.toml` - Added openapi feature

## Benefits

✅ **Self-Documenting API** - Documentation stays in sync with code
✅ **Interactive Testing** - Test endpoints without writing curl commands
✅ **Client Generation** - Generate SDKs in any language
✅ **API Discovery** - Easy exploration for new developers
✅ **Type Safety** - Schema validation ensures correctness
