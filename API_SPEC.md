# Umari REST API Specification

Complete API reference for the Umari event-sourcing system.

**Base URL:** `http://localhost:3000`

**Version:** 1.0.0

---

## Table of Contents

- [Authentication](#authentication)
- [Error Responses](#error-responses)
- [Command Execution](#command-execution)
- [Module Management](#module-management)
  - [Upload Module](#upload-module)
  - [List Modules](#list-modules)
  - [Get Module Details](#get-module-details)
  - [Get Version Details](#get-version-details)
  - [Activate Module Version](#activate-module-version)
  - [Deactivate Module](#deactivate-module)
- [Cross-Module Operations](#cross-module-operations)
- [Examples](#examples)

---

## Authentication

Currently, the API does not require authentication. All endpoints are publicly accessible.

---

## Error Responses

All error responses follow this standard format:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "optional human-readable error message"
  }
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `INVALID_INPUT` | 400 Bad Request | Invalid request format, missing fields, or validation errors |
| `NOT_FOUND` | 404 Not Found | Requested resource does not exist |
| `DUPLICATE` | 409 Conflict | Resource already exists (e.g., module version) |
| `INTEGRITY` | 422 Unprocessable Entity | Data integrity violation |
| `DATABASE` | 500 Internal Server Error | Database operation failed |
| `INTERNAL` | 500 Internal Server Error | Internal server error |

### Example Error Response

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "module not found: Command/create-account/1.0.0"
  }
}
```

---

## Command Execution

Execute a command by providing input data. The command must be uploaded and activated first.

### Execute Command

**Endpoint:** `POST /commands/{name}/execute`

**Legacy Endpoint:** `POST /execute/{name}` (deprecated)

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Command module name |

**Request Body:**

```json
{
  "input": {
    // Command-specific input structure
  }
}
```

**Response (200 OK):**

```json
{
  "position": 12345,
  "events": [
    {
      "event_type": "AccountCreated",
      "tags": ["account:acc123", "user:usr456"]
    }
  ]
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `position` | number \| null | Position in the event stream where events were written |
| `events` | array | List of events emitted by the command |
| `events[].event_type` | string | Type of the event |
| `events[].tags` | array | Domain ID tags associated with the event |

**Error Responses:**

- `404 NOT_FOUND` - Command module not found or not active
- `400 INVALID_INPUT` - Invalid input or deserialization error
- `422 INTEGRITY` - Command handler validation error
- `500 DATABASE` - Event store error

**Example:**

```bash
curl -X POST http://localhost:3000/commands/create-account/execute \
  -H "Content-Type: application/json" \
  -d '{
    "input": {
      "user_id": "usr123",
      "email": "test@example.com",
      "initial_balance": 100.0
    }
  }'
```

---

## Module Management

Manage WASM modules for commands and projectors, including upload, versioning, activation, and querying.

### Upload Module

Upload a new version of a command or projector module.

#### Upload Command Module

**Endpoint:** `POST /commands/{name}/versions/{version}`

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Module name |
| `version` | string | Yes | Semantic version (e.g., "1.0.0") |

**Query Parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `activate` | boolean | No | false | Activate module immediately after upload |

**Request Body (multipart/form-data):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `wasm` | file | Yes | WASM binary file |

**Response (201 Created):**

```json
{
  "module_type": "Command",
  "name": "create-account",
  "version": "1.0.0",
  "sha256": "abc123def456...",
  "activated": true
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `module_type` | string | Type of module ("Command" or "Projector") |
| `name` | string | Module name |
| `version` | string | Module version |
| `sha256` | string | SHA-256 hash of the WASM binary for integrity verification |
| `activated` | boolean | Whether the module was activated |

**Error Responses:**

- `400 INVALID_INPUT` - Invalid version format or missing WASM file
- `409 DUPLICATE` - Module version already exists
- `500 DATABASE` - Database error

**Example:**

```bash
# Upload and activate
curl -X POST "http://localhost:3000/commands/create-account/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/create_account.wasm"

# Upload without activating
curl -X POST http://localhost:3000/commands/create-account/versions/1.1.0 \
  -F "wasm=@target/wasm32-wasip2/release/create_account.wasm"
```

#### Upload Projector Module

**Endpoint:** `POST /projectors/{name}/versions/{version}`

Same parameters and response format as command upload.

**Example:**

```bash
curl -X POST "http://localhost:3000/projectors/accounts/versions/2.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/accounts.wasm"
```

---

### List Modules

List all modules of a specific type with their versions and activation status.

#### List Commands

**Endpoint:** `GET /commands`

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | No | Filter by module name (exact match) |
| `active_only` | boolean | No | Only return modules with at least one active version |

**Response (200 OK):**

```json
{
  "modules": [
    {
      "name": "create-account",
      "active_version": "1.0.0",
      "versions": [
        {
          "version": "1.0.0",
          "active": true,
          "sha256": "abc123..."
        },
        {
          "version": "0.9.0",
          "active": false,
          "sha256": "def456..."
        }
      ]
    }
  ]
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `modules` | array | List of modules |
| `modules[].name` | string | Module name |
| `modules[].active_version` | string \| null | Currently active version, null if none |
| `modules[].versions` | array | All versions of this module |
| `modules[].versions[].version` | string | Version string |
| `modules[].versions[].active` | boolean | Whether this version is active |
| `modules[].versions[].sha256` | string | SHA-256 hash (empty string if not available) |

**Example:**

```bash
# List all commands
curl http://localhost:3000/commands

# Filter by name
curl "http://localhost:3000/commands?name=create-account"
```

#### List Projectors

**Endpoint:** `GET /projectors`

Same parameters and response format as list commands.

**Example:**

```bash
curl http://localhost:3000/projectors
```

---

### Get Module Details

Get detailed information about a specific module, including all versions.

#### Get Command Details

**Endpoint:** `GET /commands/{name}`

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Module name |

**Response (200 OK):**

```json
{
  "module_type": "Command",
  "name": "create-account",
  "active_version": "1.0.0",
  "versions": [
    {
      "version": "1.0.0",
      "active": true,
      "sha256": "abc123..."
    },
    {
      "version": "0.9.0",
      "active": false,
      "sha256": "def456..."
    }
  ]
}
```

**Error Responses:**

- `404 NOT_FOUND` - Module does not exist

**Example:**

```bash
curl http://localhost:3000/commands/create-account
```

#### Get Projector Details

**Endpoint:** `GET /projectors/{name}`

Same parameters and response format as get command details.

**Example:**

```bash
curl http://localhost:3000/projectors/accounts
```

---

### Get Version Details

Get information about a specific version of a module.

#### Get Command Version Details

**Endpoint:** `GET /commands/{name}/versions/{version}`

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Module name |
| `version` | string | Yes | Module version |

**Response (200 OK):**

```json
{
  "module_type": "Command",
  "name": "create-account",
  "version": "1.0.0",
  "active": true,
  "sha256": "abc123..."
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `module_type` | string | Type of module |
| `name` | string | Module name |
| `version` | string | Module version |
| `active` | boolean | Whether this version is currently active |
| `sha256` | string | SHA-256 hash (empty string if not available) |

**Error Responses:**

- `404 NOT_FOUND` - Module version does not exist
- `400 INVALID_INPUT` - Invalid version format

**Example:**

```bash
curl http://localhost:3000/commands/create-account/versions/1.0.0
```

#### Get Projector Version Details

**Endpoint:** `GET /projectors/{name}/versions/{version}`

Same parameters and response format as get command version details.

**Example:**

```bash
curl http://localhost:3000/projectors/accounts/versions/2.0.0
```

---

### Activate Module Version

Activate a specific version of a module. Only one version can be active at a time. Activating a new version automatically deactivates the previous version.

#### Activate Command Version

**Endpoint:** `PUT /commands/{name}/active`

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Module name |

**Request Body:**

```json
{
  "version": "1.1.0"
}
```

**Response (200 OK):**

```json
{
  "module_type": "Command",
  "name": "create-account",
  "version": "1.1.0",
  "activated": true,
  "previous_version": "1.0.0"
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `module_type` | string | Type of module |
| `name` | string | Module name |
| `version` | string | Newly activated version |
| `activated` | boolean | Always true for successful activation |
| `previous_version` | string \| null | Previously active version, null if none |

**Error Responses:**

- `404 NOT_FOUND` - Module version does not exist
- `400 INVALID_INPUT` - Invalid version format

**Example:**

```bash
curl -X PUT http://localhost:3000/commands/create-account/active \
  -H "Content-Type: application/json" \
  -d '{"version": "1.1.0"}'
```

#### Activate Projector Version

**Endpoint:** `PUT /projectors/{name}/active`

Same parameters and response format as activate command version.

**Example:**

```bash
curl -X PUT http://localhost:3000/projectors/accounts/active \
  -H "Content-Type: application/json" \
  -d '{"version": "2.0.0"}'
```

---

### Deactivate Module

Deactivate the currently active version of a module. This is an idempotent operation.

#### Deactivate Command

**Endpoint:** `DELETE /commands/{name}/active`

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Module name |

**Response (200 OK):**

```json
{
  "module_type": "Command",
  "name": "create-account",
  "deactivated": true,
  "previous_version": "1.0.0"
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `module_type` | string | Type of module |
| `name` | string | Module name |
| `deactivated` | boolean | Always true for successful deactivation |
| `previous_version` | string \| null | Version that was deactivated, null if none was active |

**Example:**

```bash
curl -X DELETE http://localhost:3000/commands/create-account/active
```

#### Deactivate Projector

**Endpoint:** `DELETE /projectors/{name}/active`

Same parameters and response format as deactivate command.

**Example:**

```bash
curl -X DELETE http://localhost:3000/projectors/accounts/active
```

---

## Cross-Module Operations

Operations that span across multiple module types.

### List Active Modules

Get a list of all currently active modules across all types.

**Endpoint:** `GET /modules/active`

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `module_type` | string | No | Filter by module type: "command", "projector", or "effect" |

**Response (200 OK):**

```json
{
  "modules": [
    {
      "module_type": "Command",
      "name": "create-account",
      "version": "1.0.0"
    },
    {
      "module_type": "Projector",
      "name": "accounts",
      "version": "2.0.1"
    }
  ]
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `modules` | array | List of active modules |
| `modules[].module_type` | string | Type of module |
| `modules[].name` | string | Module name |
| `modules[].version` | string | Active version |

**Example:**

```bash
# List all active modules
curl http://localhost:3000/modules/active

# Filter by type
curl "http://localhost:3000/modules/active?module_type=command"
```

---

## Examples

### Complete Workflow: Deploy and Use a Command

```bash
# 1. Upload command module and activate it
curl -X POST "http://localhost:3000/commands/open-account/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/open_account.wasm"

# Response:
# {
#   "module_type": "Command",
#   "name": "open-account",
#   "version": "1.0.0",
#   "sha256": "a3f5...",
#   "activated": true
# }

# 2. Execute the command
curl -X POST http://localhost:3000/commands/open-account/execute \
  -H "Content-Type: application/json" \
  -d '{
    "input": {
      "account_id": "acc123",
      "initial_balance": 100.0
    }
  }'

# Response:
# {
#   "position": 1,
#   "events": [
#     {
#       "event_type": "OpenedAccount",
#       "tags": ["account:acc123"]
#     }
#   ]
# }

# 3. Verify module is active
curl http://localhost:3000/commands/open-account

# Response:
# {
#   "module_type": "Command",
#   "name": "open-account",
#   "active_version": "1.0.0",
#   "versions": [
#     {
#       "version": "1.0.0",
#       "active": true,
#       "sha256": "a3f5..."
#     }
#   ]
# }
```

### Update and Rollback Workflow

```bash
# 1. Upload new version (don't activate yet)
curl -X POST http://localhost:3000/commands/open-account/versions/1.1.0 \
  -F "wasm=@target/wasm32-wasip2/release/open_account_v2.wasm"

# 2. Test the new version before activating...
# (You might have a separate test environment)

# 3. Activate new version
curl -X PUT http://localhost:3000/commands/open-account/active \
  -H "Content-Type: application/json" \
  -d '{"version": "1.1.0"}'

# Response:
# {
#   "module_type": "Command",
#   "name": "open-account",
#   "version": "1.1.0",
#   "activated": true,
#   "previous_version": "1.0.0"
# }

# 4. If something goes wrong, rollback to previous version
curl -X PUT http://localhost:3000/commands/open-account/active \
  -H "Content-Type: application/json" \
  -d '{"version": "1.0.0"}'

# Response:
# {
#   "module_type": "Command",
#   "name": "open-account",
#   "version": "1.0.0",
#   "activated": true,
#   "previous_version": "1.1.0"
# }
```

### Deploy Multiple Modules

```bash
# Upload and activate multiple commands
curl -X POST "http://localhost:3000/commands/open-account/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/open_account.wasm"

curl -X POST "http://localhost:3000/commands/deposit/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/deposit.wasm"

curl -X POST "http://localhost:3000/commands/withdraw/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/withdraw.wasm"

# Upload and activate projector
curl -X POST "http://localhost:3000/projectors/accounts/versions/1.0.0?activate=true" \
  -F "wasm=@target/wasm32-wasip2/release/accounts.wasm"

# Verify all modules are active
curl http://localhost:3000/modules/active

# Response:
# {
#   "modules": [
#     {
#       "module_type": "Command",
#       "name": "open-account",
#       "version": "1.0.0"
#     },
#     {
#       "module_type": "Command",
#       "name": "deposit",
#       "version": "1.0.0"
#     },
#     {
#       "module_type": "Command",
#       "name": "withdraw",
#       "version": "1.0.0"
#     },
#     {
#       "module_type": "Projector",
#       "name": "accounts",
#       "version": "1.0.0"
#     }
#   ]
# }
```

### Module Version Management

```bash
# List all versions of a command
curl http://localhost:3000/commands/open-account

# Get details of a specific version
curl http://localhost:3000/commands/open-account/versions/1.0.0

# List all commands
curl http://localhost:3000/commands

# Deactivate a module (e.g., for maintenance)
curl -X DELETE http://localhost:3000/commands/open-account/active

# Try to execute - will return 404
curl -X POST http://localhost:3000/commands/open-account/execute \
  -H "Content-Type: application/json" \
  -d '{"input": {}}'

# Reactivate
curl -X PUT http://localhost:3000/commands/open-account/active \
  -H "Content-Type: application/json" \
  -d '{"version": "1.0.0"}'
```

---

## Notes

### Version String Format

Module versions must follow [Semantic Versioning](https://semver.org/):
- Format: `MAJOR.MINOR.PATCH` (e.g., "1.0.0")
- Optional pre-release: `1.0.0-beta.1`
- Optional build metadata: `1.0.0+build.123`

### URL Encoding

Special characters in version strings are automatically URL-encoded:
- `1.0.0-beta` → works as-is
- `1.0.0+build` → `+` becomes `%2B`
- Wrap URLs in quotes when using curl with special characters

### Idempotency

- **Upload:** Uploading the same module version twice returns `409 CONFLICT`
- **Activate:** Activating an already-active version is idempotent (returns success)
- **Deactivate:** Deactivating an already-inactive module is idempotent (returns success)

### Module Lifecycle

1. **Upload** - Store WASM binary with version
2. **Activate** - Make version available for execution
3. **Execute** - Run commands against active version
4. **Update** - Upload new version and activate
5. **Rollback** - Activate previous version if needed
6. **Deactivate** - Remove from active use (module versions remain stored)

### SHA-256 Integrity

All uploaded WASM binaries are hashed with SHA-256 for integrity verification:
- Hash is computed server-side during upload
- Returned in upload response
- Can be used to verify binary integrity
- Future enhancement: Include hash in download responses

---

## Changelog

### Version 1.0.0
- Initial API specification
- Command and projector module management
- Module upload with multipart/form-data
- Version activation and deactivation
- Command execution
- Module listing and querying
