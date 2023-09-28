use tracing_subscriber::filter::ParseError;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub fn setup_tracing(debug: bool) -> Result<(), ParseError> {
    // Layer to output to stdout
    let stdout_layer = tracing_subscriber::fmt::layer();

    let filter_level = if debug { "pywr=debug" } else { "pywr=info" };

    let filter = EnvFilter::from_default_env()
        .add_directive(filter_level.parse()?)
        // only display error logs from other crates
        .add_directive("RUST_LOG=error".parse()?);

    let subscriber = Registry::default().with(stdout_layer).with(filter);

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global tracing subscriber :(");

    Ok(())
}
