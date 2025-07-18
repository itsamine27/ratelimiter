This project delivers a custom rate-limiting middleware for use with the Tower ecosystem in Rust, including frameworks like Axum. The middleware is designed to enforce per-user quotas, allowing up to 5 requests per minute per unique user.

Each user is identified via a token x-forwarded-for header, and the middleware efficiently tracks request counts in a rolling time window.
