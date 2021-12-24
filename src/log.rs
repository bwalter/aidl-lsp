#[derive(Debug)]
pub(crate) struct LoggerFormatter;

// A simple formatter suitable for display in LSP client, copied from rust-analyzer
impl<S, N> tracing_subscriber::fmt::FormatEvent<S, N> for LoggerFormatter
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    N: for<'a> tracing_subscriber::fmt::FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: tracing_subscriber::fmt::format::Writer,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        // Write level and target
        let level = *event.metadata().level();

        // If this event is issued from `log` crate, then the value of target is
        // always "log". `tracing-log` has hard coded it for some reason, so we
        // need to extract it using `normalized_metadata` method which is part of
        // `tracing_log::NormalizeEvent`.
        //let target = match event.normalized_metadata() {
        //    // This event is issued from `log` crate
        //    Some(log) => log.target(),
        //    None => event.metadata().target(),
        //};
        let target = event.metadata().target();
        write!(writer, "[{} {}] ", level, target)?;

        // Write spans and fields of each span
        ctx.visit_spans(|span| {
            write!(writer, "{}", span.name())?;

            let ext = span.extensions();

            // `FormattedFields` is a a formatted representation of the span's
            // fields, which is stored in its extensions by the `fmt` layer's
            // `new_span` method. The fields will have been formatted
            // by the same field formatter that's provided to the event
            // formatter in the `FmtContext`.
            let fields = &ext
                .get::<tracing_subscriber::fmt::FormattedFields<N>>()
                .expect("will never be `None`");

            if !fields.is_empty() {
                write!(writer, "{{{}}}", fields)?;
            }
            write!(writer, ": ")?;

            Ok(())
        })?;

        // Write fields on the event
        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
