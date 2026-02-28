use tracing_subscriber::EnvFilter;

pub fn init() {
    // Safe to call multiple times in small prototypes; ignore error if already set.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}
