mod processes;
mod user_session;

use redis_module::redis_module;

use processes::{login::login, logout::logout, update::update};

redis_module! {
    name: "user_sessions",
    version: 0,
    allocator: (
        redis_module::alloc::RedisAlloc,
        redis_module::alloc::RedisAlloc
    ),
    data_types: [],
    commands: [
        ["session.login",  login,  "write fast deny-oom", 0, 0, 0, ""],
        ["session.logout", logout, "write fast",          0, 0, 0, ""],
        ["session.update", update, "write fast deny-oom", 0, 0, 0, ""],
    ]
}
