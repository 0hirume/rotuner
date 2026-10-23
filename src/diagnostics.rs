use std::env;

use console::Term;
use miette::{GraphicalReportHandler, GraphicalTheme};

pub(super) fn handler() -> GraphicalReportHandler {
    let terminal = Term::stderr();
    let interactive = terminal.is_term() && env::var_os("TERM").is_none_or(|value| value != "dumb");

    let theme = match (interactive, console::colors_enabled_stderr()) {
        (true, true) => GraphicalTheme::unicode(),
        (true, false) => GraphicalTheme::unicode_nocolor(),
        (false, true) => GraphicalTheme::ascii(),
        (false, false) => GraphicalTheme::none(),
    };

    GraphicalReportHandler::new_themed(theme)
        .with_width(usize::from(terminal.size().1).max(20))
        .with_links(false)
        .with_wrap_lines(false)
}
