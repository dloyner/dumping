use std::time::Duration;

use redis::{AsyncCommands, AsyncConnectionConfig, Client};
use wasmtime::{Engine, Linker, Module, Store, bail};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p1::{self, WasiP1Ctx};

use kernel::Payload;

const QUEUE: &str = "payload_queue";

#[tokio::main]
async fn main() {
    let client = Client::open("redis://127.0.0.1/").expect("Failed to create Redis client");
    let (engine, linker) = build_runtime().expect("Failed to build Wasmtime runtime");
    worker(client, engine, linker).await;
}

fn build_runtime() -> wasmtime::Result<(Engine, Linker<WasiP1Ctx>)> {
    let engine = Engine::default();
    let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
    p1::add_to_linker_async(&mut linker, |ctx| ctx)?;
    Ok((engine, linker))
}

async fn worker(client: Client, engine: Engine, linker: Linker<WasiP1Ctx>) {
    let config = AsyncConnectionConfig::new().set_response_timeout(None);
    let mut connection = client
        .get_multiplexed_async_connection_with_config(&config)
        .await
        .expect("Failed to connect to Redis");

    println!("Worker started. Listening for messages on '{QUEUE}'...");

    loop {
        let popped: Option<(String, String)> = match connection.blpop(QUEUE, 0.0).await {
            Ok(popped) => popped,
            Err(err) => {
                eprintln!("Error reading from redis: {err}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let Some((_queue_name, raw)) = popped else {
            continue;
        };

        let payload = match serde_json::from_str::<Payload>(&raw) {
            Ok(payload) => payload,
            Err(err) => {
                eprintln!("Failed to deserialize JSON: {err}");
                continue;
            }
        };

        let (updated_payload, outcome) = match run_worker(&engine, &linker, payload).await {
            Ok(result) => result,
            Err(err) => {
                eprintln!("Failed to run worker: {err}");
                continue;
            }
        };

        if outcome == 1 {
            println!("Task completed");
            continue;
        }

        let json = match serde_json::to_string(&updated_payload) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("Failed to serialize json: {err}");
                continue;
            }
        };

        match connection.rpush::<_, _, ()>(QUEUE, json).await {
            Ok(()) => println!("task not completed"),
            Err(e) => eprintln!("Failed to rqueue payload: {e}"),
        }
    }
}

async fn run_worker(
    engine: &Engine,
    linker: &Linker<WasiP1Ctx>,
    payload: Payload,
) -> wasmtime::Result<(Payload, i32)> {
    let module = Module::new(engine, &payload.contract)?;
    let pre = linker.instantiate_pre(&module)?;

    let wasi_ctx = WasiCtxBuilder::new()
        .inherit_stdin()
        .inherit_env()
        .build_p1();

    let mut store = Store::new(engine, wasi_ctx);
    let instance = pre.instantiate_async(&mut store).await?;

    let Ok(len) = u32::try_from(payload.memory.len()) else {
        bail!("payload memory is too large");
    };

    let malloc = instance.get_typed_func::<(u32, u32), u32>(&mut store, "ns_malloc")?;
    let ptr = malloc.call_async(&mut store, (len, 1)).await?;
    if ptr == 0 {
        bail!("guest allocation failed");
    }

    let Some(memory) = instance.get_memory(&mut store, "memory") else {
        bail!("guest does not export `memory`");
    };
    memory.write(&mut store, ptr as usize, &payload.memory)?;

    let run_compute = instance.get_typed_func::<(u32, u32), i32>(&mut store, "run_compute")?;
    let ret = run_compute.call_async(&mut store, (ptr, len)).await?;

    let mut updated_memory = vec![0u8; payload.memory.len()];
    memory.read(&store, ptr as usize, &mut updated_memory)?;

    let updated = Payload {
        memory: updated_memory,
        contract: payload.contract,
    };

    Ok((updated, ret))
}
