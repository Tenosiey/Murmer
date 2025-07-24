# Murmer Server Guide

This folder hosts the Rust WebSocket server built with Axum, providing chat and voice coordination services.

## Architecture Overview
- **Framework**: Axum for HTTP/WebSocket handling
- **Database**: PostgreSQL for persistent message storage
- **Authentication**: Ed25519 signature verification with anti-replay protection
- **Security**: Rate limiting, input validation, CORS protection
- **File Handling**: Secure image upload with type validation and size limits

## Environment Variables
- `DATABASE_URL` - PostgreSQL connection string (required)
- `UPLOAD_DIR` - Directory for file uploads (default: "uploads")
- `SERVER_PASSWORD` - Optional password for client authentication
- `ADMIN_TOKEN` - Bearer token for administrative role management

## API Endpoints
- `GET /` - Health check endpoint
- `WebSocket /ws` - Main chat and voice coordination
- `POST /upload` - Image upload with security validation
- `POST /role` - Admin endpoint for user role management
- `GET /files/*` - Static file serving for uploaded images

## WebSocket Message Types
- `presence` - User authentication and registration
- `chat` - Text messages with optional images
- `switch-channel` - Change active text channel
- `load-history` - Request message history (limited to 200 messages)
- `create-channel`/`delete-channel` - Channel management
- `create-voice-channel`/`delete-voice-channel` - Voice channel management
- `voice-*` - Voice channel join/leave operations

## Security Features
- **Ed25519 Authentication**: Cryptographic signatures for user verification
- **Replay Protection**: Nonce-based system prevents signature reuse
- **Rate Limiting**: Configurable limits on auth attempts and messages
- **Input Validation**: Channel names, usernames, and content validation
- **File Security**: Type checking, size limits, and atomic writes
- **Admin Protection**: Constant-time token comparison for admin operations

## Database Schema
- `messages` - Chat message storage with channel association
- `roles` - User role assignments with color customization
- `channels` - Text channel registry
- `voice_channels` - Voice channel registry

## Running
The recommended way to run the server is using Docker Compose from the repository root:

```bash
docker compose up --build
```

The server listens on `ws://localhost:3001/ws` and HTTP on `http://localhost:3001`.

## Development Guidelines
- All database operations should handle errors gracefully
- Input validation must be performed on all user-provided data
- Rate limiting should be tested under load
- Security features should be documented with SECURITY comments
- Use structured logging for security events

## Security Considerations
- Private keys are never stored on the server
- All timestamps are validated to prevent replay attacks
- File uploads are restricted and validated by content type
- Rate limiting prevents abuse and DoS attacks
- Admin tokens use constant-time comparison to prevent timing attacks

## Validation
- Run `cargo check` to verify the code builds
- Format code with `cargo fmt` before committing
- Test WebSocket connections and authentication flows
- Verify rate limiting under concurrent connections
- Test file upload security restrictions
- There are no automated tests - this should be added for production use
