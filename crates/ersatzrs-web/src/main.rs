const TOKIO_WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == ersatzrs_web::host::tool_recovery::RECOVERY_ARGUMENT)
    {
        match ersatzrs_web::host::tool_recovery::run(std::env::args_os().skip(2)) {
            Ok(count) => println!("Retired {count} selected tool receipt(s). No executable or media files were changed."),
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(1);
            }
        }
        return;
    }
    let preparation_worker = std::env::args_os().nth(1).is_some_and(|argument| {
        argument == ersatzrs_web::host::preparation_worker::PRIVATE_WORKER_ARGUMENT
    });
    if preparation_worker {
        let result = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_stack_size(TOKIO_WORKER_STACK_SIZE)
            .build()
            .expect("failed to build preparation worker runtime")
            .block_on(ersatzrs_web::host::preparation_worker::run_private_worker());
        if result.is_err() {
            eprintln!("add-on preparation worker requires recovery");
            std::process::exit(1);
        }
        return;
    }
    // Own the state folder before migrations, runtime acquisition or workers.
    // The private preparation worker/recovery command above has separate leases.
    let _instance_guard = match ersatzrs_platform::single_instance::Guard::acquire(
        &ersatzrs_infra::file_system_layout::app_state_folder(),
    ) {
        Ok(guard) => Some(guard),
        Err(error @ ersatzrs_platform::single_instance::Error::AlreadyRunning) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
        Err(error) => {
            // Match upstream's non-contention fallback; logging is not ready yet.
            eprintln!("Warning: {error}; continuing without the single-instance check");
            None
        }
    };
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TOKIO_WORKER_STACK_SIZE)
        .build()
        .expect("failed to build Tokio runtime")
        .block_on(ersatzrs_web::run());
}
