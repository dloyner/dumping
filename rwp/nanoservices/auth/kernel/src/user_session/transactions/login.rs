use std::future::Future;

use cache_client::login as cache_login;
use glue::errors::NanoServiceError;

use crate::user_session::descriptors::RedisSessionDescriptor;

pub trait LoginUserSession {
    fn login_user_session(
        &self,
        address: &str,
        user_id: &str,
        timeout_mins: usize,
        perm_user_id: i32,
    ) -> impl Future<Output = Result<(), NanoServiceError>> + Send;
}

#[cfg(feature = "core-postgres")]
impl<T> LoginUserSession for RedisSessionDescriptor<T> {
    fn login_user_session(
        &self,
        address: &str,
        user_id: &str,
        timeout_mins: usize,
        perm_user_id: i32,
    ) -> impl Future<Output = Result<(), NanoServiceError>> + Send {
        cache_login(address, user_id, timeout_mins, perm_user_id)
    }
}

#[cfg(all(feature = "http", not(feature = "core-postgres")))]
impl LoginUserSession for RedisSessionDescriptor {
    fn login_user_session(
        &self,
        address: &str,
        user_id: &str,
        timeout_mins: usize,
        perm_user_id: i32,
    ) -> impl Future<Output = Result<(), NanoServiceError>> + Send {
        cache_login(address, user_id, timeout_mins, perm_user_id)
    }
}
