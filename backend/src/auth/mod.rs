mod entra;
mod provider;

pub use entra::EntraAuthenticator;
pub use provider::{
    AuthMode, AuthenticationFailure, Authenticator, DevelopmentAuthenticator, Principal,
    build_authenticator,
};
