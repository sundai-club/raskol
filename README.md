### Project Structure

- `src/main.rs`: CLI and application entry point
- `src/server.rs`: Server implementation and request handling
- `src/auth.rs`: JWT authentication logic
- `src/jwt.rs`: JWT token management
- `src/server/tests.rs`: Integration tests

## Security

- All API requests require valid JWT tokens
- Rate limiting is enforced per user
- JWT tokens include expiration time
- Groq API key is never exposed to clients

## License

[Insert your chosen license here]
