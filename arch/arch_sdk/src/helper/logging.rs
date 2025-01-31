/* -------------------------------------------------------------------------- */
/*                          LOGGING HELPER FUNCTIONS                          */
/* -------------------------------------------------------------------------- */
/// Logging helper functions for pretty prints ✨✨
pub const DEFAULT_LOG_LEVEL: &str = "info";

pub fn print_title(title: &str, color: u8) {
    let termsize::Size { rows: _, cols } =
        termsize::get().unwrap_or(termsize::Size { rows: 24, cols: 80 });
    let term_width = usize::from(cols);

    let color_code = match color {
        1 => 34, // Blue
        2 => 33, // Yellow
        3 => 31, // Red
        4 => 36, // Cyan
        _ => 32, // Green (default)
    };

    let start_format = format!("\x1b[1m\x1b[{}m", color_code);
    let reset_format = "\x1b[0m";

    let line = format!("===== {} ", title);
    let remaining_width = term_width.saturating_sub(line.len());
    let dashes = "=".repeat(remaining_width);

    println!("{}{}{}{}", start_format, line, dashes, reset_format);
}

pub fn log_scenario_start(scenario_index: u16, scenario_title: &str, scenario_description: &str) {
    println!("\n\n\n");

    // Print header separator
    print_title("", 1); // Blue separator line

    // Print scenario title
    println!(
        "\x1b[1m\x1b[36m===== Scenario {} : \x1b[0m\x1b[1m {} \x1b[36m=====\x1b[0m",
        scenario_index, scenario_title
    );

    print_title("", 1); // Blue separator line

    // Print description section
    println!(
        "\x1b[1m\x1b[3m\x1b[36m=====\x1b[0m \x1b[1m\x1b[3m {} \x1b[0m",
        scenario_description
    );
    // Print footer separator
    print_title("", 1); // Blue separator line
}

pub fn log_scenario_end(scenario_index: u16, scenario_states: &str) {
    println!();

    // Print end separator
    print_title("", 1); // Blue separator line

    // Print scenario end message
    println!(
        "\x1b[1m\x1b[32m===== Scenario {} Finished Successfully! \x1b[0m\x1b[1m Final state: {} \x1b[32m=====\x1b[0m",
        scenario_index, scenario_states
    );

    // Print footer separator
    print_title("", 1); // Blue separator line
}

pub fn init_logging() {
    use std::{env, sync::Once};

    static INIT: Once = Once::new();

    INIT.call_once(|| {
        if env::var("RUST_LOG").is_err() {
            env::set_var("RUST_LOG", DEFAULT_LOG_LEVEL);
        }

        tracing_subscriber::fmt()
            .without_time()
            .with_file(false)
            .with_line_number(false)
            .with_env_filter(tracing_subscriber::EnvFilter::new(format!(
                "{},reqwest=off,hyper=off",
                env::var("RUST_LOG").unwrap()
            )))
            .init();
    });
}
