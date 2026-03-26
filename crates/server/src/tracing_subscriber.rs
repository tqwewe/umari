use std::fmt;
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format},
    registry::LookupSpan,
};

pub struct PrettyNoSpans;

impl<S, N> FormatEvent<S, N> for PrettyNoSpans
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let ansi = writer.has_ansi_escapes();

        if ansi {
            let color = match *meta.level() {
                Level::TRACE => "\x1b[35m",
                Level::DEBUG => "\x1b[34m",
                Level::INFO  => "\x1b[32m",
                Level::WARN  => "\x1b[1m\x1b[33m",
                Level::ERROR => "\x1b[1m\x1b[31m",
            };
            write!(writer, "{}{:>5}\x1b[0m ", color, meta.level())?;
        } else {
            write!(writer, "{:>5} ", meta.level())?;
        }

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        write!(writer, "{}", visitor.message)?;

        match visitor.fields.len() {
            0 => {}
            1 => {
                let (key, val) = &visitor.fields[0];
                if ansi {
                    write!(writer, "  \x1b[2m{key}=\x1b[0m{val}")?;
                } else {
                    write!(writer, "  {key}={val}")?;
                }
            }
            _ => {
                let max_key = visitor.fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
                for (key, val) in &visitor.fields {
                    if ansi {
                        write!(writer, "\n      \x1b[2m{key:<max_key$}\x1b[0m  {val}")?;
                    } else {
                        write!(writer, "\n      {key:<max_key$}  {val}")?;
                    }
                }
            }
        }

        writeln!(writer)
    }
}

#[derive(Default)]
struct EventVisitor {
    message: String,
    fields: Vec<(String, String)>,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields
                .push((field.name().to_owned(), format!("{value:?}")));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            self.fields
                .push((field.name().to_owned(), value.to_owned()));
        }
    }
}
