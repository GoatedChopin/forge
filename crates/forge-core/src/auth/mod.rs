//! Authentication and JWT handling.
//!
//! Forge supports JWT authentication with both symmetric (HMAC) and asymmetric
//! (RSA via JWKS) algorithms. Tokens are validated on every request and claims
//! are available in the function context.
//!
//! # Configuration
//!
//! ```toml
//! [auth]
//! jwt_algorithm = "RS256"
//! jwks_url = "https://provider.com/.well-known/jwks.json"
//! jwt_issuer = "https://provider.com"
//! jwt_audience = "my-app"
//! ```
//!
//! # Accessing Auth in Functions
//!
//! ```ignore
//! #[forge::query]
//! async fn get_profile(ctx: &QueryContext) -> Result<User> {
//!     let user_id = ctx.auth.require_user_id()?;
//!     // ...
//! }
//! ```
//!
//! # Non-UUID Providers
//!
//! For providers like Firebase or Clerk that don't use UUID subjects:
//!
//! ```ignore
//! let subject = ctx.auth.require_subject()?; // Returns &str
//! ```
//!
//! # Key Types
//!
//! - [`Claims`] - Parsed JWT claims
//! - [`ClaimsBuilder`] - Builder for constructing claims (testing)

mod claims;

pub use claims::{Claims, ClaimsBuilder};
