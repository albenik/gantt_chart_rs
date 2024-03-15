use core::fmt::Arguments;

use colored::Colorize;
use gantt::{
    error,
    GanttChartLog,
    GanttChartTool,
};

struct GanttChartLogger;

impl GanttChartLogger {
    fn new() -> GanttChartLogger {
        GanttChartLogger {}
    }
}

impl GanttChartLog for GanttChartLogger {
    fn output(&self, args: Arguments) {
        println!("{}", args);
    }
    fn warning(&self, args: Arguments) {
        eprintln!("{}", format!("warning: {}", args).yellow());
    }
    fn error(&self, args: Arguments) {
        eprintln!("{}", format!("error: {}", args).red());
    }
}

fn main() {
    let logger = GanttChartLogger::new();

    if let Err(error) = GanttChartTool::new(&logger).run(std::env::args_os()) {
        error!(logger, "{}", error);
        std::process::exit(1);
    }
}
