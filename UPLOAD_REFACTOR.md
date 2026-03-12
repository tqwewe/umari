# Upload Endpoint Refactoring

## Before (Form Fields)

```bash
POST /commands
  -F "name=create-account"
  -F "version=1.0.0"
  -F "wasm=@create-account.wasm"
  -F "activate=true"
```

**Issues:**
- Metadata in form fields instead of URL
- Less RESTful
- Inconsistent with other endpoints
- URL doesn't show what resource you're creating

## After (URL Parameters) ✨

```bash
POST /commands/create-account/versions/1.0.0?activate=true
  -F "wasm=@create-account.wasm"
```

**Benefits:**
- ✅ Fully RESTful - resource hierarchy is clear
- ✅ Consistent with other endpoints (`GET /commands/{name}/versions/{version}`)
- ✅ URL shows exactly what you're uploading
- ✅ Only the binary payload in form data
- ✅ Cleaner separation of concerns

## Complete Examples

### Upload and Activate Immediately
```bash
curl -X POST "http://localhost:3000/commands/create-account/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/create_account.wasm"
```

### Upload Without Activating
```bash
curl -X POST http://localhost:3000/commands/create-account/versions/1.1.0 \
  -F "wasm=@target/wasm32-wasip2/release/create_account.wasm"
```

### Upload Projection
```bash
curl -X POST "http://localhost:3000/projections/accounts/versions/2.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/accounts.wasm"
```

## URL Encoding Note

Special characters in version strings are automatically URL-encoded by curl:
- `1.0.0-beta` → works as-is
- `1.0.0+build.123` → `+` becomes `%2B`
- Just wrap the URL in quotes if it contains special characters

## API Changes

**New Endpoints:**
- `POST /commands/{name}/versions/{version}?activate={bool}` (was `POST /commands`)
- `POST /projections/{name}/versions/{version}?activate={bool}` (was `POST /projections`)

**Request Format:**
- Only `wasm` field in multipart form data
- `name` and `version` come from URL path
- `activate` is an optional query parameter
