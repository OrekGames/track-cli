use crate::cli::OutputFormat;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::io::{BufWriter, IsTerminal, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tracker_core::{
    Article, ArticleAttachment, BundleDefinition, Comment, CustomField, CustomFieldDefinition,
    Issue, IssueAttachment, IssueHistoryEvent, IssueTag, Project, ProjectCustomField, case_key,
    unicode_eq_ignore_case,
};

/// Process-wide JSON render mode, set once from `-o` before dispatch. JSONL is
/// a rendering of the document `-o json` prints, so handlers stay exhaustive
/// on Text/Json and only [`output_json`] needs to know.
static JSONL_MODE: AtomicBool = AtomicBool::new(false);

/// Process-wide text render mode, set once from `-o agent`. Like JSONL for
/// JSON, it is a rendering of the text handlers already print: compact,
/// uncoloured, and never cut short without saying so.
static AGENT_MODE: AtomicBool = AtomicBool::new(false);

/// Render text output for an AI agent instead of a person at a terminal.
pub fn set_agent_mode(enabled: bool) {
    AGENT_MODE.store(enabled, Ordering::Relaxed);
}

/// Whether text output is being rendered for an AI agent.
pub fn is_agent() -> bool {
    AGENT_MODE.load(Ordering::Relaxed)
}

/// The variable that picks the output format when `-o` is not given. The
/// session hooks and `track agent install` set it to [`OUTPUT_ENV_VALUE`].
pub(crate) const OUTPUT_ENV: &str = "TRACK_OUTPUT";
/// What agent setup sets [`OUTPUT_ENV`] to: `-o agent`.
pub(crate) const OUTPUT_ENV_VALUE: &str = "agent";

/// `--select`, set once before dispatch. Every JSON document a command
/// prints goes through [`output_json`], so projecting there covers them all.
static SELECTION: OnceLock<Selection> = OnceLock::new();

struct Selection {
    paths: Vec<Vec<String>>,
    /// Whether a selection that matches nothing fails the command.
    must_match: bool,
}

/// Project every JSON document onto these paths.
///
/// With `must_match`, a selection that matches nothing is an error: an empty
/// object would read as "this issue has no such data". A command that
/// writes passes false. Its write is done by the time its output is
/// projected, and failing then would invite a retry that writes twice, so a
/// miss prints the whole document with a warning instead.
pub fn set_select_paths(paths: Vec<Vec<String>>, must_match: bool) {
    let _ = SELECTION.set(Selection { paths, must_match });
}

/// Render every JSON document as compact JSON Lines instead of pretty JSON.
pub fn set_jsonl_mode(enabled: bool) {
    JSONL_MODE.store(enabled, Ordering::Relaxed);
}

/// Print `value` as pretty JSON, or as JSONL under `-o jsonl`.
pub fn output_json<T: Serialize + ?Sized>(value: &T) -> anyhow::Result<()> {
    let jsonl = JSONL_MODE.load(Ordering::Relaxed);
    // Without `--select` the value is written as is, so key order and
    // formatting stay exactly what they have always been.
    let Some(selection) = SELECTION.get() else {
        return print_json(value, jsonl);
    };
    let (projected, matched) = select_from(serde_json::to_value(value)?, &selection.paths);
    if matched {
        return print_json(&projected, jsonl);
    }
    let missed = no_match_message(&selection.paths);
    if selection.must_match {
        return Err(
            crate::errors::CliError::new(crate::errors::CliCode::SelectNoMatch, missed)
                .with_hint("Run the command once without --select to see the field names.")
                .into(),
        );
    }
    output_verification_warnings(
        &[format!(
            "{missed}. The command succeeded; its whole JSON output follows, with the field names to select from."
        )],
        OutputFormat::Json,
    );
    print_json(value, jsonl)
}

fn no_match_message(paths: &[Vec<String>]) -> String {
    let shown: Vec<String> = paths.iter().map(|path| path.join(".")).collect();
    format!(
        "--select matched nothing: no '{}' in this command's JSON output",
        shown.join("', '")
    )
}

/// One line of a JSON Lines stream that a command writes itself, projected
/// like everything else when `--select` is set.
pub fn jsonl_line<T: Serialize + ?Sized>(value: &T) -> anyhow::Result<String> {
    match SELECTION.get() {
        None => Ok(serde_json::to_string(value)?),
        // One line of many: a line the paths do not reach (a failure
        // record, say) is not a reason to stop the stream.
        Some(selection) => Ok(serde_json::to_string(
            &select_from(serde_json::to_value(value)?, &selection.paths).0,
        )?),
    }
}

/// Project `value` onto `--select` paths, and say whether anything matched.
///
/// A path segment named `fields` reaches a flat view of an issue's custom
/// fields (`fields.Priority`), computed here so the default output does not
/// carry it. An empty list matches: nothing in it was missed.
fn select_from(mut value: Value, paths: &[Vec<String>]) -> (Value, bool) {
    if paths
        .iter()
        .any(|path| path.iter().any(|segment| segment == FLAT_FIELDS_KEY))
    {
        add_flat_fields(&mut value);
    }
    let projected = project_value(&value, paths);
    let matched_nothing = match (&value, &projected) {
        (Value::Array(source), Value::Array(items)) => {
            !source.is_empty() && items.iter().all(is_empty_object)
        }
        (_, projected) => is_empty_object(projected),
    };
    (projected, !matched_nothing)
}

fn is_empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(Map::is_empty)
}

/// Key of the flat custom-field view `--select` can address.
const FLAT_FIELDS_KEY: &str = "fields";

/// Give every object that has `custom_fields` a `fields` map of name to
/// plain value, wherever it sits in the document.
fn add_flat_fields(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(add_flat_fields),
        Value::Object(map) => {
            map.values_mut().for_each(add_flat_fields);
            if let Some(Value::Array(custom_fields)) = map.get("custom_fields")
                && !map.contains_key(FLAT_FIELDS_KEY)
            {
                let flat: Map<String, Value> =
                    custom_fields.iter().filter_map(flat_field).collect();
                map.insert(FLAT_FIELDS_KEY.to_string(), Value::Object(flat));
            }
        }
        _ => {}
    }
}

/// `{"SingleEnum": {"name": "Priority", "value": "Major"}}` as
/// `("Priority", "Major")`. A multi-value field is an array, an untyped one
/// its raw JSON, and a field with no value `null`: it is still a field the
/// issue has, so selecting it is not a miss.
fn flat_field(field: &Value) -> Option<(String, Value)> {
    let field = CustomField::deserialize(field).ok()?;
    let value = match &field {
        CustomField::MultiEnum { values, .. } => Value::from(values.clone()),
        CustomField::Unknown { value, .. } => value.clone().unwrap_or(Value::Null),
        other => other.value_text().map_or(Value::Null, Value::String),
    };
    Some((field.name().to_string(), value))
}

fn print_json<T: Serialize + ?Sized>(value: &T, jsonl: bool) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let handle = stdout.lock();
    let mut writer = BufWriter::new(handle);
    write_json(&mut writer, value, jsonl)?;
    writer.flush()?;
    Ok(())
}

/// Write `value` as pretty JSON, or as compact JSONL when `jsonl` is set.
///
/// JSONL is root-aware: a top-level array emits one compact record per
/// element (nothing for an empty array); a top-level object (or other value)
/// emits one compact line. Nested arrays stay inside their record.
fn write_json<W: Write, T: Serialize + ?Sized>(
    writer: &mut W,
    value: &T,
    jsonl: bool,
) -> anyhow::Result<()> {
    if !jsonl {
        serde_json::to_writer_pretty(&mut *writer, value)?;
        writeln!(writer)?;
        return Ok(());
    }
    let compact = serde_json::to_vec(value)?;
    for record in jsonl_records(&compact) {
        writer.write_all(record)?;
        writeln!(writer)?;
    }
    Ok(())
}

/// Split compact JSON into JSONL records: the elements of a top-level array,
/// or the whole document otherwise. Splitting the serialized bytes instead of
/// going through [`Value`] keeps each record's key order identical to `-o json`.
fn jsonl_records(compact: &[u8]) -> Vec<&[u8]> {
    let Some(inner) = compact
        .strip_prefix(b"[")
        .and_then(|rest| rest.strip_suffix(b"]"))
    else {
        return vec![compact];
    };
    let mut records = Vec::new();
    if inner.is_empty() {
        return records;
    }
    // Compact output has no whitespace, so a comma outside any string and
    // nested container ends an element.
    let (mut depth, mut in_string, mut escaped, mut start) = (0usize, false, false, 0);
    for (i, &byte) in inner.iter().enumerate() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                records.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    records.push(&inner[start..]);
    records
}

/// Output verification warnings to stderr.
///
/// Every format writes to stderr, so JSON on stdout stays parseable.
pub fn output_verification_warnings(warnings: &[String], _format: OutputFormat) {
    for warning in warnings {
        eprintln!("{} {}", "⚠ Warning:".yellow().bold(), warning);
    }
}

pub fn output_result<T: Serialize + Displayable>(
    result: &T,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            output_json(result)?;
        }
        OutputFormat::Text if is_agent() => {
            println!("{}", result.display_agent());
        }
        OutputFormat::Text => {
            println!("{}", result.display());
        }
    }
    Ok(())
}

pub fn output_list<T: Serialize + Displayable>(
    items: &[T],
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            output_json(items)?;
        }
        OutputFormat::Text if is_agent() => {
            if let Some(header) = T::agent_list_header().filter(|_| !items.is_empty()) {
                println!("{header}");
            }
            for item in items {
                println!("{}", item.display_agent_row());
            }
        }
        OutputFormat::Text => {
            for item in items {
                println!("{}", item.display());
                println!();
            }
        }
    }
    Ok(())
}

/// Parse `--select` as comma-separated dotted paths. Trim and skip empties.
///
/// Double quotes keep `.` and `,` inside a name: `fields."Est. time"`.
pub(crate) fn parse_select_paths(select: &str) -> Vec<Vec<String>> {
    fn end_segment(segment: &mut String, was_quoted: &mut bool, path: &mut Vec<String>) {
        let text = if *was_quoted {
            segment.clone()
        } else {
            segment.trim().to_string()
        };
        if !text.is_empty() {
            path.push(text);
        }
        segment.clear();
        *was_quoted = false;
    }

    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut segment = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    for ch in select.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                was_quoted = true;
            }
            '.' if !quoted => end_segment(&mut segment, &mut was_quoted, &mut path),
            ',' if !quoted => {
                end_segment(&mut segment, &mut was_quoted, &mut path);
                if !path.is_empty() {
                    paths.push(std::mem::take(&mut path));
                }
            }
            _ => segment.push(ch),
        }
    }
    end_segment(&mut segment, &mut was_quoted, &mut path);
    if !path.is_empty() {
        paths.push(path);
    }
    paths
}

/// Project a single dotted path from `value`.
///
/// Missing paths yield `None` (omitted; no `--strict-select` in this slice).
/// When the walk hits an array, the remaining path is projected from each
/// element at its source index so sparse sibling fields do not collapse.
/// Empty arrays stay empty. A miss on one element is a JSON `null` hole so
/// later paths can merge by index.
///
/// Paths bind to serialized field names as-is. This slice does not unwrap
/// externally tagged objects.
pub(crate) fn project_one(value: &Value, path: &[String]) -> Option<Value> {
    if path.is_empty() {
        return Some(value.clone());
    }
    match value {
        Value::Object(map) => {
            let child = map.get(&path[0])?;
            let projected = project_one(child, &path[1..])?;
            let mut obj = Map::new();
            obj.insert(path[0].clone(), projected);
            Some(Value::Object(obj))
        }
        Value::Array(items) => {
            let projected: Vec<Value> = items
                .iter()
                .map(|item| project_one(item, path).unwrap_or(Value::Null))
                .collect();
            Some(Value::Array(projected))
        }
        _ => None,
    }
}

/// Deep-merge objects; merge arrays element-wise by source index.
///
/// JSON `null` is a hole: it does not overwrite a present value, and a
/// present value fills a hole.
pub(crate) fn merge_projected(dst: &mut Value, src: Value) {
    if src.is_null() {
        return;
    }
    if dst.is_null() {
        *dst = src;
        return;
    }
    match (dst, src) {
        (Value::Object(dst_map), Value::Object(src_map)) => {
            for (key, src_val) in src_map {
                if let Some(existing) = dst_map.get_mut(&key) {
                    merge_projected(existing, src_val);
                } else {
                    dst_map.insert(key, src_val);
                }
            }
        }
        (Value::Array(dst_items), Value::Array(src_items)) => {
            if dst_items.len() < src_items.len() {
                dst_items.resize(src_items.len(), Value::Null);
            }
            for (i, src_item) in src_items.into_iter().enumerate() {
                merge_projected(&mut dst_items[i], src_item);
            }
        }
        (dst, src) => {
            *dst = src;
        }
    }
}

/// Project `value` onto the given dotted paths.
///
/// A top-level array is projected per element (the issue list). Nested
/// arrays keep source length, order, and indices.
pub(crate) fn project_value(value: &Value, paths: &[Vec<String>]) -> Value {
    if let Value::Array(items) = value {
        return Value::Array(
            items
                .iter()
                .map(|item| project_value(item, paths))
                .collect(),
        );
    }
    let mut out = Value::Object(Map::new());
    for path in paths {
        if let Some(piece) = project_one(value, path) {
            merge_projected(&mut out, piece);
        }
    }
    out
}

/// Output a summary of changes made to an issue.
pub fn output_change_summary(
    old: Option<&Issue>,
    new: &Issue,
    update: Option<&tracker_core::UpdateIssue>,
    create: Option<&tracker_core::CreateIssue>,
    format: OutputFormat,
) {
    if format != OutputFormat::Text {
        return;
    }

    println!("{}", "--- Change Summary ---".bright_black());

    // 1. Identify which fields were requested. Keys are Unicode-folded via
    // `case_key` so non-ASCII custom field names match consistently with
    // `unicode_eq_ignore_case` used by `find_field_value`.
    let mut requested_fields: HashSet<String> = HashSet::new();
    if let Some(u) = update {
        if u.summary.is_some() {
            requested_fields.insert(case_key("Summary"));
        }
        if u.description.is_some() {
            requested_fields.insert(case_key("Description"));
        }
        if u.parent.is_some() {
            requested_fields.insert(case_key("Parent"));
        }
        for f in &u.custom_fields {
            requested_fields.insert(case_key(resolve_requested_field_name(f, new)));
        }
    }
    if let Some(c) = create {
        requested_fields.insert(case_key("Summary"));
        if c.description.is_some() {
            requested_fields.insert(case_key("Description"));
        }
        if c.parent.is_some() {
            requested_fields.insert(case_key("Parent"));
        }
        for f in &c.custom_fields {
            requested_fields.insert(case_key(resolve_requested_field_name(f, new)));
        }
    }

    // 2. Identify all detected changes
    let mut displayed_fields = HashSet::new();

    // Check Summary
    if old.map(|o| &o.summary) != Some(&new.summary) {
        display_change(
            "Summary",
            old.map(|o| o.summary.as_str()),
            Some(&new.summary),
            &requested_fields,
        );
    }
    displayed_fields.insert("summary".to_string());

    // Check Description
    if old.and_then(|o| o.description.as_deref()) != new.description.as_deref() {
        display_change(
            "Description",
            old.and_then(|o| o.description.as_deref()),
            new.description.as_deref(),
            &requested_fields,
        );
    }
    displayed_fields.insert("description".to_string());

    // Parent relationships aren't carried on the core Issue struct (they live
    // in custom_fields or links, backend-specific), so we can't diff them here.
    // Mark "parent" as considered so a request for `--parent X` doesn't get
    // misreported as "Ignored" when the underlying API call actually succeeded.
    displayed_fields.insert(case_key("Parent"));

    // Check Custom Fields
    for new_cf in &new.custom_fields {
        let name = new_cf.name();

        let old_val = old.and_then(|o| find_field_value(o, name));
        let new_val = find_field_value(new, name);

        if old_val != new_val {
            display_change(
                name,
                old_val.as_deref(),
                new_val.as_deref(),
                &requested_fields,
            );
        }
        displayed_fields.insert(case_key(name));
    }

    // Check for ignored fields (requested but not in new or not changed)
    for req_field in &requested_fields {
        if !displayed_fields.contains(req_field) {
            // Requested but not displayed -> likely ignored
            println!("  {} ({})", req_field.bold(), "Ignored".yellow().dimmed());
        }
    }
}

fn display_change(name: &str, old: Option<&str>, new: Option<&str>, requested: &HashSet<String>) {
    let is_requested = requested.contains(&case_key(name));
    let prefix = if is_requested { "" } else { "(Side Effect) " };
    let label = if is_requested {
        name.bold()
    } else {
        name.bright_black()
    };

    match (old, new) {
        (Some(o), Some(n)) if o != n => {
            println!(
                "  {}{}: {} -> {}",
                prefix.dimmed(),
                label,
                o.dimmed(),
                n.green()
            );
        }
        (None, Some(n)) => {
            println!("  {}{}: {}", prefix.dimmed(), label, n.green());
        }
        (Some(o), None) => {
            println!(
                "  {}{}: {} -> {}",
                prefix.dimmed(),
                label,
                o.dimmed(),
                "None".red()
            );
        }
        _ => {}
    }
}

/// Resolve a requested custom field update to the field name the issue actually
/// carries. A State update targets the tracker's workflow state field whatever the
/// backend names it (YouTrack "State"/"Stage", Jira "Status"): when the requested
/// name doesn't appear on the issue but the issue has exactly one State-typed field,
/// the request resolves to that field so the change summary attributes it correctly
/// instead of reporting "(Side Effect)" plus a bogus "Ignored" row.
fn resolve_requested_field_name<'a>(
    requested: &'a tracker_core::CustomFieldUpdate,
    issue: &'a Issue,
) -> &'a str {
    let name = requested.name();

    let name_exists = issue
        .custom_fields
        .iter()
        .any(|f| unicode_eq_ignore_case(f.name(), name));
    if !matches!(requested, tracker_core::CustomFieldUpdate::State { .. }) || name_exists {
        return name;
    }

    let mut states = issue.custom_fields.iter().filter_map(|f| match f {
        CustomField::State { name, .. } => Some(name.as_str()),
        _ => None,
    });
    match (states.next(), states.next()) {
        (Some(state_name), None) => state_name,
        _ => name,
    }
}

fn find_field_value(issue: &Issue, name: &str) -> Option<String> {
    issue
        .custom_fields
        .iter()
        .find(|f| unicode_eq_ignore_case(f.name(), name))
        .and_then(CustomField::value_text)
}

/// Report a failure on stderr: what went wrong, its stable code, and the
/// next step when one is known.
///
/// Returns the report, whose exit code is the process's.
pub fn output_error(
    err: &anyhow::Error,
    format: crate::errors::ErrorFormat,
) -> crate::errors::ErrorReport {
    let report = crate::errors::describe(err);
    eprintln!("{}", crate::errors::render(&report, format, true));
    report
}

/// Advice for a request that failed because the server's certificate does
/// not chain to a trusted root, as happens behind a proxy that inspects HTTPS.
pub fn untrusted_certificate_tip(error: &str) -> Option<String> {
    if !error.contains("UnknownIssuer") {
        return None;
    }
    let mut tip = String::from(
        "The server's certificate is not issued by a certificate authority track trusts. \
         If a proxy inspects your HTTPS traffic, set SSL_CERT_FILE to its CA certificate \
         (PEM), or run `track config set system_certs true` to also trust your operating \
         system's certificates.",
    );
    for error in tracker_core::tls::load_errors() {
        tip.push_str(&format!("\nCould not load certificates: {error}"));
    }
    Some(tip)
}

/// Output a progress message to stderr if stdout is a TTY and format is text
pub fn output_progress(message: &str, format: OutputFormat) {
    if format == OutputFormat::Text && std::io::stdout().is_terminal() {
        eprintln!("{} {}", "→".cyan().bold(), message);
    }
}

/// How many results a search has in all, when that is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageTotal {
    /// Counted by the tracker for this request.
    Live(u64),
    /// Counted earlier and cached; `age` says when ("2h ago").
    Cached { count: u64, age: String },
}

/// Where the next page starts, or `None` when this page is the whole
/// result: `--all` was given, or the page is short of its limit.
fn next_page_skip(shown: usize, limit: usize, skip: usize, fetched_all: bool) -> Option<usize> {
    (!fetched_all && shown > 0 && shown >= limit).then_some(skip + shown)
}

/// Say whether a list is the whole result and, when it may not be, how to
/// get the rest.
///
/// `-o agent` puts the verdict on the last line of stdout, complete or not.
/// Every other format gets a hint on stderr, and only for a full page: JSON
/// too, since a full page of JSON looks exactly like a complete result.
pub fn output_page_hint(
    shown: usize,
    limit: usize,
    skip: usize,
    total: Option<PageTotal>,
    fetched_all: bool,
) {
    let next_skip = next_page_skip(shown, limit, skip, fetched_all);
    if is_agent() {
        println!("{}", agent_page_footer(shown, next_skip, total.as_ref()));
    } else if let Some(next_skip) = next_skip {
        eprintln!("{}", page_hint(shown, skip, next_skip, total.as_ref()));
    }
}

/// The stderr hint for a full page.
fn page_hint(shown: usize, skip: usize, next_skip: usize, total: Option<&PageTotal>) -> String {
    let total_part = match total {
        Some(PageTotal::Live(total)) => format!(" ({next_skip} of {total} total)"),
        Some(PageTotal::Cached { count, age }) => format!(" (~{count} total, {age})"),
        None => String::new(),
    };
    let skip_flag = format!("--skip {next_skip}").cyan();
    // `--all` conflicts with `--skip` in clap, so once paging with --skip,
    // only --skip is suggested.
    let how = if skip == 0 {
        format!("use {} or {skip_flag} for next page", "--all".cyan())
    } else {
        format!("use {skip_flag} for next page")
    };
    format!(
        "  {} {shown} results shown{}  ·  {how}",
        "┄┄".dimmed(),
        total_part.dimmed()
    )
}

/// The last line of an agent-mode list: whether it is the whole result and,
/// if not, how to get the rest.
fn agent_page_footer(shown: usize, next_skip: Option<usize>, total: Option<&PageTotal>) -> String {
    let Some(next_skip) = next_skip else {
        return format!("# {shown} shown, complete");
    };
    let of_total = match total {
        Some(PageTotal::Live(total)) => format!(" of {total}"),
        Some(PageTotal::Cached { count, age }) => format!(" of ~{count} (counted {age})"),
        None => String::new(),
    };
    format!(
        "# {shown} shown{of_total}; more may exist: --skip {next_skip}, or --all for everything"
    )
}

pub trait Displayable {
    fn display(&self) -> String;

    /// The `-o agent` rendering: as few bytes as carry the whole record.
    /// Defaults to the terminal rendering, which is uncoloured in that mode.
    fn display_agent(&self) -> String {
        self.display()
    }

    /// One item of an `-o agent` list. Defaults to [`Self::display_agent`];
    /// a record whose detail view is long gives a shorter row, and the item
    /// is a `get` away.
    fn display_agent_row(&self) -> String {
        self.display_agent()
    }

    /// The line above an `-o agent` list whose rows are columns.
    fn agent_list_header() -> Option<&'static str>
    where
        Self: Sized,
    {
        None
    }
}

/// `Name: value` for a custom field that has a value.
fn agent_field(field: &CustomField) -> Option<String> {
    let value = field.value_text().filter(|value| !value.is_empty())?;
    let resolved = matches!(
        field,
        CustomField::State {
            is_resolved: true,
            ..
        }
    );
    Some(if resolved {
        format!("{}: {value} (resolved)", field.name())
    } else {
        format!("{}: {value}", field.name())
    })
}

/// Header of the rows [`agent_issue_row`] prints.
pub const AGENT_ISSUE_HEADER: &str = "# id\tstate\tpriority\tassignee\tsummary";

/// One issue on one line, for lists: id, state, priority, assignee, summary,
/// tab-separated, with `-` for a value the issue does not have.
pub fn agent_issue_row(issue: &Issue) -> String {
    let fields = issue.common_fields();
    let or_dash = |value: Option<&String>| value.map_or("-", String::as_str).to_string();
    [
        issue.id_readable.clone(),
        or_dash(fields.state),
        or_dash(fields.priority),
        or_dash(fields.assignee),
        // A summary is one line by nature; make sure a stray tab or newline
        // cannot break the row.
        issue.summary.replace(['\t', '\n', '\r'], " "),
    ]
    .join("\t")
}

/// The acknowledgement of a write: what was done to which issue, and the
/// field values it ended up with. The description is not echoed back.
pub fn agent_issue_ack(verb: &str, issue: &Issue) -> String {
    let mut parts = vec![format!("{verb} {}  {}", issue.id_readable, issue.summary)];
    parts.extend(issue.custom_fields.iter().filter_map(agent_field));
    parts.join(" | ")
}

/// Longest single value an agent-facing list prints. Longer ones are cut
/// with an ellipsis.
pub(crate) const MAX_VALUE_CHARS: usize = 60;

/// Make a string safe to print on one line of an agent's context.
///
/// Cached values can come from anywhere a `.tracker-cache/` directory can: a
/// tracker anyone may write to, or a cloned repository that committed one. So
/// control characters (newlines included) become spaces, runs of whitespace
/// collapse, and long values are cut.
pub(crate) fn clean(value: &str, max_chars: usize) -> String {
    let mut cleaned = String::with_capacity(value.len());
    let mut last_was_space = true;
    for ch in value.chars() {
        let ch = if ch.is_control() || ch.is_whitespace() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if !last_was_space {
                cleaned.push(' ');
            }
            last_was_space = true;
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }
    let cleaned = cleaned.trim_end();
    if cleaned.chars().count() <= max_chars {
        return cleaned.to_string();
    }
    let mut cut: String = cleaned.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// A list cut to a budget, with the number of entries left out. Used
/// wherever a list of names is printed for an agent, so every such list
/// reads the same.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Capped {
    pub shown: Vec<String>,
    /// Entries not shown. Never dropped silently: the text names a command
    /// that lists them.
    pub more: usize,
}

impl Capped {
    pub(crate) fn new<I: IntoIterator<Item = String>>(items: I, max: usize) -> Self {
        let mut shown: Vec<String> = Vec::new();
        let mut more = 0;
        for item in items {
            if shown.len() < max {
                shown.push(item);
            } else {
                more += 1;
            }
        }
        Self { shown, more }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }

    /// `a, b, … +N more (hint)`, each entry made safe to print.
    pub(crate) fn render(&self, hint: &str) -> String {
        let mut line = self
            .shown
            .iter()
            .map(|item| clean(item, MAX_VALUE_CHARS))
            .collect::<Vec<_>>()
            .join(", ");
        if self.more > 0 {
            line.push_str(&format!(", … +{} more ({})", self.more, hint));
        }
        line
    }
}

impl Displayable for Issue {
    fn display(&self) -> String {
        let mut output = format!(
            "{} - {}\n  {}: {}\n  {}: {}\n  {}: {}",
            self.id_readable.cyan().bold(),
            self.summary.white().bold(),
            "Project".dimmed(),
            self.project
                .short_name
                .as_deref()
                .unwrap_or(&self.project.id),
            "Created".dimmed(),
            self.created
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed(),
            "Updated".dimmed(),
            self.updated
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed()
        );

        if let Some(resolved) = &self.resolved {
            output.push_str(&format!(
                "\n  {}: {}",
                "Resolved".dimmed(),
                resolved.format("%Y-%m-%d %H:%M:%S").to_string().dimmed()
            ));
        }

        if let Some(desc) = &self.description {
            output.push_str(&format!("\n  {}: {}", "Description".dimmed(), desc));
        }

        if !self.tags.is_empty() {
            let tag_names: Vec<String> = self
                .tags
                .iter()
                .map(|t| t.name.magenta().to_string())
                .collect();
            output.push_str(&format!(
                "\n  {}: {}",
                "Tags".dimmed(),
                tag_names.join(", ")
            ));
        }

        if !self.custom_fields.is_empty() {
            output.push_str(&format!("\n  {}:", "Custom Fields".dimmed()));
            for field in &self.custom_fields {
                output.push_str(&format!("\n    {}", field.display()));
            }
        }

        output
    }

    fn display_agent(&self) -> String {
        let mut output = format!("{}  {}", self.id_readable, self.summary);

        let fields: Vec<String> = self.custom_fields.iter().filter_map(agent_field).collect();
        if !fields.is_empty() {
            output.push('\n');
            output.push_str(&fields.join(" | "));
        }

        let mut meta = vec![format!(
            "project: {}",
            self.project
                .short_name
                .as_deref()
                .unwrap_or(&self.project.id)
        )];
        if !self.tags.is_empty() {
            let names: Vec<&str> = self.tags.iter().map(|t| t.name.as_str()).collect();
            meta.push(format!("tags: {}", names.join(", ")));
        }
        meta.push(format!("created: {}", self.created.format("%Y-%m-%d")));
        meta.push(format!("updated: {}", self.updated.format("%Y-%m-%d")));
        if let Some(resolved) = &self.resolved {
            meta.push(format!("resolved: {}", resolved.format("%Y-%m-%d")));
        }
        output.push('\n');
        output.push_str(&meta.join(" | "));

        if let Some(description) = self.description.as_deref().map(str::trim_end)
            && !description.trim().is_empty()
        {
            output.push_str("\ndescription:\n");
            output.push_str(description);
        }

        output
    }

    fn display_agent_row(&self) -> String {
        agent_issue_row(self)
    }

    fn agent_list_header() -> Option<&'static str> {
        Some(AGENT_ISSUE_HEADER)
    }
}

impl Displayable for CustomField {
    fn display(&self) -> String {
        match self {
            CustomField::SingleEnum { name, value } => {
                let val = value.as_deref().unwrap_or("None");
                let colored_val = colorize_priority(name, val);
                format!("{}: {}", name.dimmed(), colored_val)
            }
            CustomField::State {
                name,
                value,
                is_resolved,
            } => {
                let val = value.as_deref().unwrap_or("None");
                let colored_val = if *is_resolved {
                    val.green().to_string()
                } else {
                    val.to_string()
                };
                format!("{}: {}", name.dimmed(), colored_val)
            }
            CustomField::SingleUser { name, login, .. } => {
                format!("{}: {}", name.dimmed(), login.as_deref().unwrap_or("None"))
            }
            CustomField::Text { name, value } => {
                format!("{}: {}", name.dimmed(), value.as_deref().unwrap_or("None"))
            }
            CustomField::MultiEnum { name, values } => {
                format!("{}: {}", name.dimmed(), values.join(", "))
            }
            CustomField::Unknown { name, value } => {
                let rendered = match value {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => "Unknown field".dimmed().to_string(),
                };
                format!("{}: {}", name.dimmed(), rendered)
            }
        }
    }
}

fn colorize_priority(field_name: &str, value: &str) -> String {
    if field_name.eq_ignore_ascii_case("priority") {
        match value.to_lowercase().as_str() {
            "critical" | "show-stopper" => value.red().bold().to_string(),
            "major" | "high" => value.red().to_string(),
            "minor" | "low" => value.dimmed().to_string(),
            _ => value.to_string(),
        }
    } else {
        value.to_string()
    }
}

impl Displayable for Project {
    fn display(&self) -> String {
        let mut output = format!(
            "{} ({}) - {}",
            self.short_name.cyan().bold(),
            self.id.dimmed(),
            self.name.white().bold()
        );
        if let Some(desc) = &self.description {
            output.push_str(&format!("\n  {}: {}", "Description".dimmed(), desc));
        }
        output
    }
}

impl Displayable for ProjectCustomField {
    fn display(&self) -> String {
        let required = if self.required {
            " (required)".yellow().to_string()
        } else {
            String::new()
        };
        format!(
            "{} [{}]{}",
            self.name.white().bold(),
            self.field_type.dimmed(),
            required
        )
    }

    fn display_agent(&self) -> String {
        let mut kind = self.field_type.clone();
        if self.required {
            kind.push_str(", required");
        }
        if self.values.is_empty() {
            return format!("{} ({})", self.name, kind);
        }
        // Resolved states are marked, as in `track prime`.
        let values: Vec<String> = self
            .values
            .iter()
            .map(|value| {
                let resolved = self
                    .state_values
                    .iter()
                    .any(|state| state.is_resolved && &state.name == value);
                if resolved {
                    format!("{value}*")
                } else {
                    value.clone()
                }
            })
            .collect();
        format!("{} ({}): {}", self.name, kind, values.join(", "))
    }
}

impl Displayable for IssueTag {
    fn display(&self) -> String {
        format!("{} ({})", self.name.magenta(), self.id.dimmed())
    }
}

impl Displayable for Article {
    fn display(&self) -> String {
        let mut output = format!(
            "{} - {}\n  {}: {}\n  {}: {}\n  {}: {}",
            self.id_readable.cyan().bold(),
            self.summary.white().bold(),
            "Project".dimmed(),
            self.project
                .short_name
                .as_deref()
                .unwrap_or(&self.project.id),
            "Created".dimmed(),
            self.created
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed(),
            "Updated".dimmed(),
            self.updated
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed()
        );

        if let Some(parent) = &self.parent_article {
            output.push_str(&format!(
                "\n  {}: {}",
                "Parent".dimmed(),
                parent.id_readable.as_deref().unwrap_or(&parent.id).cyan()
            ));
        }

        if self.has_children {
            output.push_str(&format!(
                "\n  {}: {}",
                "Has children".dimmed(),
                "yes".green()
            ));
        }

        if !self.tags.is_empty() {
            let tag_names: Vec<String> = self
                .tags
                .iter()
                .map(|t| t.name.magenta().to_string())
                .collect();
            output.push_str(&format!(
                "\n  {}: {}",
                "Tags".dimmed(),
                tag_names.join(", ")
            ));
        }

        if let Some(content) = &self.content {
            let mut chars = content.chars();
            let preview: String = chars.by_ref().take(ARTICLE_PREVIEW_CHARS).collect();
            // Count chars, not bytes: multi-byte text within the limit is shown whole.
            let ellipsis = if chars.next().is_some() { "..." } else { "" };
            output.push_str(&format!(
                "\n  {}: {}{}",
                "Content".dimmed(),
                preview,
                ellipsis
            ));
        }

        output
    }

    fn display_agent(&self) -> String {
        let mut output = format!("{}  {}", self.id_readable, self.summary);
        output.push('\n');
        output.push_str(&article_agent_meta(self).join(" | "));
        // The whole content: the terminal view's 200-character preview would
        // make an agent fetch the article a second time as JSON.
        if let Some(content) = self.content.as_deref().map(str::trim_end)
            && !content.trim().is_empty()
        {
            output.push_str("\ncontent:\n");
            output.push_str(content);
        }
        output
    }

    /// One line, without the content: a list of whole articles is a list
    /// of documents. `article get` prints one in full.
    fn display_agent_row(&self) -> String {
        format!(
            "{}  {} | {}",
            self.id_readable,
            self.summary.replace(['\t', '\n', '\r'], " "),
            article_agent_meta(self).join(" | ")
        )
    }
}

/// An article's project, place in the tree, tags and dates, for `-o agent`.
fn article_agent_meta(article: &Article) -> Vec<String> {
    let mut meta = vec![format!(
        "project: {}",
        article
            .project
            .short_name
            .as_deref()
            .unwrap_or(&article.project.id)
    )];
    if let Some(parent) = &article.parent_article {
        meta.push(format!(
            "parent: {}",
            parent.id_readable.as_deref().unwrap_or(&parent.id)
        ));
    }
    if article.has_children {
        meta.push("has children".to_string());
    }
    if !article.tags.is_empty() {
        let names: Vec<&str> = article.tags.iter().map(|t| t.name.as_str()).collect();
        meta.push(format!("tags: {}", names.join(", ")));
    }
    meta.push(format!("created: {}", article.created.format("%Y-%m-%d")));
    meta.push(format!("updated: {}", article.updated.format("%Y-%m-%d")));
    meta
}

/// Maximum number of article content characters shown in text output.
const ARTICLE_PREVIEW_CHARS: usize = 200;

/// Render an attachment size in bytes, KB, or MB.
fn format_attachment_size(size: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = 1024 * KB;
    if size > MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size > KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{} bytes", size)
    }
}

impl Displayable for ArticleAttachment {
    fn display(&self) -> String {
        format!(
            "{} ({}) - {}",
            self.name.white().bold(),
            self.mime_type.as_deref().unwrap_or("unknown").dimmed(),
            format_attachment_size(self.size).dimmed()
        )
    }
}

impl Displayable for IssueAttachment {
    fn display(&self) -> String {
        let mut output = format!(
            "{} ({}) - {}",
            self.name.white().bold(),
            self.mime_type.as_deref().unwrap_or("unknown").dimmed(),
            format_attachment_size(self.size).dimmed()
        );

        if let Some(comment_id) = &self.comment_id {
            output.push_str(&format!("\n  {}: {}", "Comment".dimmed(), comment_id));
        }

        if let Some(url) = &self.url {
            output.push_str(&format!("\n  {}: {}", "URL".dimmed(), url));
        }

        if let Some(markdown) = &self.markdown {
            output.push_str(&format!("\n  {}: {}", "Markdown".dimmed(), markdown));
        }

        output
    }
}

impl Displayable for Comment {
    fn display(&self) -> String {
        let author = self
            .author
            .as_ref()
            .map(|a| a.name.as_deref().unwrap_or(&a.login))
            .unwrap_or("Unknown");

        let date = self
            .created
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Unknown date".to_string());

        format!("[{}] {} - {}", date.dimmed(), author.cyan(), self.text)
    }

    fn display_agent(&self) -> String {
        let author = self
            .author
            .as_ref()
            .map(|a| a.login.as_str())
            .unwrap_or("unknown");
        let date = self
            .created
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown date".to_string());
        format!("[{}] {}: {}", date, author, self.text.trim_end())
    }
}

impl Displayable for IssueHistoryEvent {
    fn display(&self) -> String {
        let author = self
            .author
            .as_ref()
            .map(|a| a.name.as_deref().unwrap_or(&a.login))
            .unwrap_or("Unknown");

        let date = self.at.format("%Y-%m-%d %H:%M").to_string();
        let from = self.from.as_deref().unwrap_or("∅");
        let to = self.to.as_deref().unwrap_or("∅");

        format!(
            "[{}] {} {}: {} → {}",
            date.dimmed(),
            author.cyan(),
            self.field.yellow(),
            from,
            to
        )
    }
}

impl Displayable for CustomFieldDefinition {
    fn display(&self) -> String {
        format!(
            "{} [{}] ({})",
            self.name.white().bold(),
            self.field_type.dimmed(),
            self.id.dimmed()
        )
    }
}

impl Displayable for BundleDefinition {
    fn display(&self) -> String {
        let values: Vec<&str> = self.values.iter().map(|v| v.name.as_str()).collect();
        let values_str = if values.is_empty() {
            "(no values)".dimmed().to_string()
        } else {
            values.join(", ")
        };

        format!(
            "{} [{}] ({})\n  {}: {}",
            self.name.white().bold(),
            self.bundle_type.dimmed(),
            self.id.dimmed(),
            "Values".dimmed(),
            values_str
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` without the colour codes `colored` may have added.
    fn plain(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                chars.by_ref().find(|&c| c == 'm');
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn only_a_full_page_that_was_not_fetched_whole_has_a_next_page() {
        assert_eq!(next_page_skip(7, 20, 0, false), None);
        assert_eq!(next_page_skip(0, 0, 0, false), None);
        assert_eq!(next_page_skip(20, 20, 0, false), Some(20));
        assert_eq!(next_page_skip(20, 20, 40, false), Some(60));
        assert_eq!(next_page_skip(500, 20, 0, true), None);
    }

    #[test]
    fn page_hint_names_the_next_page_and_any_total() {
        assert_eq!(
            plain(&page_hint(20, 0, 20, None)),
            "  ┄┄ 20 results shown  ·  use --all or --skip 20 for next page"
        );
        // --all conflicts with --skip, so a later page suggests --skip only.
        assert_eq!(
            plain(&page_hint(20, 40, 60, Some(&PageTotal::Live(97)))),
            "  ┄┄ 20 results shown (60 of 97 total)  ·  use --skip 60 for next page"
        );
        let cached = PageTotal::Cached {
            count: 847,
            age: "2h ago".to_string(),
        };
        assert_eq!(
            plain(&page_hint(20, 0, 20, Some(&cached))),
            "  ┄┄ 20 results shown (~847 total, 2h ago)  ·  use --all or --skip 20 for next page"
        );
    }

    fn issue_with_field(field_name: &str, value: &str) -> Issue {
        Issue {
            id: "1".into(),
            id_readable: "PROJ-1".into(),
            summary: "test".into(),
            description: None,
            project: tracker_core::ProjectRef {
                id: "p1".into(),
                name: Some("Project".into()),
                short_name: Some("PROJ".into()),
            },
            custom_fields: vec![CustomField::Text {
                name: field_name.into(),
                value: Some(value.into()),
            }],
            tags: vec![],
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            resolved: None,
        }
    }

    #[test]
    fn find_field_value_matches_non_ascii_case_insensitively() {
        let issue = issue_with_field("Geöffnet", "value");
        assert_eq!(
            find_field_value(&issue, "GEÖFFNET"),
            Some("value".to_string())
        );
        assert_eq!(
            find_field_value(&issue, "geöffnet"),
            Some("value".to_string())
        );
        assert_eq!(
            find_field_value(&issue, "Geöffnet"),
            Some("value".to_string())
        );
    }

    #[test]
    fn find_field_value_misses_when_truly_different() {
        let issue = issue_with_field("Geöffnet", "value");
        assert_eq!(find_field_value(&issue, "Geschlossen"), None);
    }

    fn issue_with_custom_fields(custom_fields: Vec<CustomField>) -> Issue {
        Issue {
            custom_fields,
            ..issue_with_field("unused", "unused")
        }
    }

    #[test]
    fn issue_display_renders_components_multi_enum() {
        // Jira's Components field is surfaced as a MultiEnum custom field named
        // "Components"; the text output should list the component names.
        let issue = issue_with_custom_fields(vec![CustomField::MultiEnum {
            name: "Components".into(),
            values: vec!["Rendering".into(), "Audio".into()],
        }]);
        let rendered = issue.display();
        assert!(
            rendered.contains("Components"),
            "expected Components label in output, got:\n{rendered}"
        );
        assert!(
            rendered.contains("Rendering, Audio"),
            "expected joined component names in output, got:\n{rendered}"
        );
    }

    fn state_update(name: &str) -> tracker_core::CustomFieldUpdate {
        tracker_core::CustomFieldUpdate::State {
            name: name.into(),
            value: "Done".into(),
        }
    }

    #[test]
    fn state_request_resolves_to_renamed_state_field() {
        // Jira names its state field "Status"; a `--state` request arrives as "State"
        let issue = issue_with_custom_fields(vec![CustomField::State {
            name: "Status".into(),
            value: Some("Done".into()),
            is_resolved: true,
        }]);
        assert_eq!(
            resolve_requested_field_name(&state_update("State"), &issue),
            "Status"
        );
    }

    #[test]
    fn state_request_keeps_name_when_it_exists_on_issue() {
        let issue = issue_with_custom_fields(vec![
            CustomField::State {
                name: "Stage".into(),
                value: Some("Done".into()),
                is_resolved: true,
            },
            CustomField::State {
                name: "State".into(),
                value: Some("Open".into()),
                is_resolved: false,
            },
        ]);
        assert_eq!(
            resolve_requested_field_name(&state_update("Stage"), &issue),
            "Stage"
        );
    }

    #[test]
    fn state_request_keeps_name_when_state_fields_ambiguous() {
        let issue = issue_with_custom_fields(vec![
            CustomField::State {
                name: "Stage".into(),
                value: Some("Done".into()),
                is_resolved: true,
            },
            CustomField::State {
                name: "Status".into(),
                value: Some("Open".into()),
                is_resolved: false,
            },
        ]);
        assert_eq!(
            resolve_requested_field_name(&state_update("Phase"), &issue),
            "Phase"
        );
    }

    #[test]
    fn change_summary_with_renamed_state_field_smoke() {
        // Jira shape: `--state Done` requests "State", the issue carries "Status".
        // Exercises the full summary path (display assertions aren't capturable here;
        // the name resolution itself is covered by the tests above).
        let old = issue_with_custom_fields(vec![CustomField::State {
            name: "Status".into(),
            value: Some("New".into()),
            is_resolved: false,
        }]);
        let new = issue_with_custom_fields(vec![CustomField::State {
            name: "Status".into(),
            value: Some("Done".into()),
            is_resolved: true,
        }]);
        let update = tracker_core::UpdateIssue {
            custom_fields: vec![state_update("State")],
            ..Default::default()
        };
        output_change_summary(Some(&old), &new, Some(&update), None, OutputFormat::Text);
    }

    #[test]
    fn non_state_request_never_resolves_to_state_field() {
        let issue = issue_with_custom_fields(vec![CustomField::State {
            name: "Status".into(),
            value: Some("Done".into()),
            is_resolved: true,
        }]);
        let requested = tracker_core::CustomFieldUpdate::SingleEnum {
            name: "Component".into(),
            value: "UI".into(),
        };
        assert_eq!(
            resolve_requested_field_name(&requested, &issue),
            "Component"
        );
    }

    fn article_with_content(content: String) -> Article {
        Article {
            id: "1".into(),
            id_readable: "KB-A-1".into(),
            summary: "test".into(),
            content: Some(content),
            project: tracker_core::ProjectRef {
                id: "p1".into(),
                name: None,
                short_name: Some("KB".into()),
            },
            parent_article: None,
            has_children: false,
            tags: vec![],
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            reporter: None,
        }
    }

    #[test]
    fn article_display_keeps_multibyte_content_within_limit_whole() {
        // 150 chars but 300 bytes: under the char limit, so no ellipsis.
        let content = "ж".repeat(150);
        let rendered = article_with_content(content.clone()).display();
        assert!(rendered.contains(&content), "got:\n{rendered}");
        assert!(!rendered.contains("..."), "got:\n{rendered}");
    }

    #[test]
    fn article_display_truncates_content_over_limit() {
        let rendered = article_with_content("ж".repeat(ARTICLE_PREVIEW_CHARS + 1)).display();
        let preview = "ж".repeat(ARTICLE_PREVIEW_CHARS);
        assert!(
            rendered.contains(&format!("{preview}...")),
            "got:\n{rendered}"
        );
    }

    #[test]
    fn attachment_size_uses_bytes_kb_and_mb() {
        assert_eq!(format_attachment_size(512), "512 bytes");
        assert_eq!(format_attachment_size(1024), "1024 bytes");
        assert_eq!(format_attachment_size(1536), "1.5 KB");
        assert_eq!(format_attachment_size(3 * 1024 * 1024 / 2), "1.5 MB");
    }

    #[test]
    fn project_object_id_readable_and_summary() {
        let value = serde_json::json!({
            "id_readable": "DEMO-1",
            "summary": "Implement user authentication",
            "description": "should be dropped"
        });
        let paths = parse_select_paths("id_readable,summary");
        let projected = project_value(&value, &paths);
        let obj = projected.as_object().expect("projected object");
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["id_readable"], "DEMO-1");
        assert_eq!(obj["summary"], "Implement user authentication");
        assert!(!obj.contains_key("description"));
    }

    #[test]
    fn project_array_child_keeps_array_of_objects() {
        let value = serde_json::json!({
            "items": [
                {"name": "State", "value": "Open"},
                {"name": "Priority", "value": "Normal"}
            ]
        });
        let paths = parse_select_paths("items.name");
        let projected = project_value(&value, &paths);
        assert_eq!(
            projected,
            serde_json::json!({
                "items": [
                    {"name": "State"},
                    {"name": "Priority"}
                ]
            })
        );
    }

    #[test]
    fn project_missing_path_is_omitted() {
        let value = serde_json::json!({
            "id_readable": "DEMO-1",
            "summary": "Hello"
        });
        let paths = parse_select_paths("id_readable,not_a_field");
        let projected = project_value(&value, &paths);
        let obj = projected.as_object().expect("projected object");
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["id_readable"], "DEMO-1");
        assert!(!obj.contains_key("not_a_field"));
    }

    #[test]
    fn project_dotted_nest_rebuilds_object() {
        let value = serde_json::json!({
            "id_readable": "DEMO-1",
            "project": {
                "id": "0-1",
                "short_name": "DEMO",
                "name": "Demo"
            }
        });
        let paths = parse_select_paths("project.short_name");
        let projected = project_value(&value, &paths);
        assert_eq!(
            projected,
            serde_json::json!({
                "project": {
                    "short_name": "DEMO"
                }
            })
        );
    }

    #[test]
    fn project_sparse_array_fields_keep_source_indices() {
        // filter_map + index merge collapses [{"a":1},{"b":2}] into one object.
        // Projection must keep source length, order, and indices.
        let value = serde_json::json!({
            "items": [{"a": 1}, {"b": 2}]
        });
        let paths = parse_select_paths("items.a,items.b");
        let projected = project_value(&value, &paths);
        assert_eq!(
            projected,
            serde_json::json!({
                "items": [{"a": 1}, {"b": 2}]
            })
        );
    }

    #[test]
    fn project_empty_array_stays_empty() {
        let value = serde_json::json!({
            "items": []
        });
        let paths = parse_select_paths("items.a");
        let projected = project_value(&value, &paths);
        assert_eq!(
            projected,
            serde_json::json!({
                "items": []
            })
        );
    }

    fn render<T: Serialize + ?Sized>(value: &T, jsonl: bool) -> String {
        let mut out = Vec::new();
        write_json(&mut out, value, jsonl).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn json_mode_pretty_prints() {
        let value = serde_json::json!({"a": [1, 2]});
        assert_eq!(
            render(&value, false),
            "{\n  \"a\": [\n    1,\n    2\n  ]\n}\n"
        );
    }

    #[test]
    fn jsonl_array_root_emits_one_record_per_element() {
        // Strings hold the separators the splitter keys on; nested arrays and
        // objects stay inside their record.
        let value = serde_json::json!([
            {"id": "A", "tags": ["x", "y"], "note": "a,b]}"},
            {"id": "B", "note": "quote \" and backslash \\", "nested": [[], {}]},
            "plain, string",
            7,
            null
        ]);
        let rendered = render(&value, true);
        let lines: Vec<&str> = rendered.lines().collect();
        let items = value.as_array().unwrap();
        assert_eq!(lines.len(), items.len(), "{rendered}");
        for (line, item) in lines.iter().zip(items) {
            assert_eq!(&serde_json::from_str::<Value>(line).unwrap(), item);
        }
    }

    #[test]
    fn jsonl_empty_array_emits_nothing() {
        assert_eq!(render(&serde_json::json!([]), true), "");
    }

    #[test]
    fn jsonl_non_array_root_emits_one_line() {
        assert_eq!(
            render(&serde_json::json!({"items": [1, 2], "ok": true}), true),
            "{\"items\":[1,2],\"ok\":true}\n"
        );
        assert_eq!(render(&serde_json::json!("[x]"), true), "\"[x]\"\n");
        assert_eq!(render(&serde_json::json!(null), true), "null\n");
    }

    #[test]
    fn jsonl_keeps_struct_field_order() {
        // Going through `Value` would sort the keys alphabetically.
        #[derive(Serialize)]
        struct Record {
            zeta: u8,
            alpha: u8,
        }
        let records = [Record { zeta: 1, alpha: 2 }, Record { zeta: 3, alpha: 4 }];
        assert_eq!(
            render(&records[..], true),
            "{\"zeta\":1,\"alpha\":2}\n{\"zeta\":3,\"alpha\":4}\n"
        );
    }

    #[test]
    fn select_paths_split_on_dots_and_commas_outside_quotes() {
        assert_eq!(
            parse_select_paths(" id_readable , project.short_name ,, "),
            [vec!["id_readable"], vec!["project", "short_name"]]
        );
        assert_eq!(
            parse_select_paths(r#"fields."Est. time",fields."A, B".x"#),
            [vec!["fields", "Est. time"], vec!["fields", "A, B", "x"]]
        );
        assert!(parse_select_paths(" , . ").is_empty());
    }

    #[test]
    fn flat_fields_are_added_wherever_custom_fields_are() {
        let mut doc = serde_json::json!({
            "issue": {"custom_fields": [
                {"State": {"name": "Stage", "value": "Done", "is_resolved": true}},
                {"SingleEnum": {"name": "Priority", "value": null}},
                {"SingleUser": {"name": "Assignee", "login": "jdoe", "display_name": "J"}},
                {"MultiEnum": {"name": "Components", "values": ["A", "B"]}},
                {"Unknown": {"name": "Sprint", "value": [{"id": 12}]}}
            ]},
            "issues": [{"custom_fields": []}]
        });
        add_flat_fields(&mut doc);
        assert_eq!(
            doc["issue"]["fields"],
            serde_json::json!({
                "Stage": "Done", "Priority": null, "Assignee": "jdoe",
                "Components": ["A", "B"], "Sprint": [{"id": 12}]
            })
        );
        assert_eq!(doc["issues"][0]["fields"], serde_json::json!({}));
    }

    #[test]
    fn selecting_a_field_with_no_value_is_a_match() {
        // An unassigned issue: the field is there, its value is not.
        let doc = serde_json::json!({"custom_fields": [
            {"SingleUser": {"name": "Assignee", "login": null, "display_name": null}},
            {"Unknown": {"name": "Sprint"}}
        ]});
        let (projected, matched) = select_from(doc.clone(), &parse_select_paths("fields.Assignee"));
        assert!(matched);
        assert_eq!(projected, serde_json::json!({"fields": {"Assignee": null}}));
        let (projected, matched) = select_from(doc.clone(), &parse_select_paths("fields.Sprint"));
        assert!(matched);
        assert_eq!(projected, serde_json::json!({"fields": {"Sprint": null}}));
        // A name the issue does not have is still a miss.
        assert!(!select_from(doc, &parse_select_paths("fields.Asignee")).1);
    }

    #[test]
    fn select_reports_whether_anything_matched() {
        let paths = parse_select_paths("nope");
        let (projected, matched) = select_from(serde_json::json!({"id": 1}), &paths);
        assert!(!matched);
        assert_eq!(projected, serde_json::json!({}));
        assert!(select_from(serde_json::json!({"id": 1}), &parse_select_paths("id")).1);
        // An empty list selects to an empty list: nothing was missed.
        assert_eq!(
            select_from(serde_json::json!([]), &paths),
            (serde_json::json!([]), true)
        );
    }

    #[test]
    fn agent_footer_says_complete_or_how_to_continue() {
        let footer = |shown, limit, skip, total: Option<PageTotal>, all| {
            agent_page_footer(
                shown,
                next_page_skip(shown, limit, skip, all),
                total.as_ref(),
            )
        };
        assert_eq!(footer(7, 20, 0, None, false), "# 7 shown, complete");
        assert_eq!(footer(0, 20, 0, None, false), "# 0 shown, complete");
        assert_eq!(footer(500, 20, 0, None, true), "# 500 shown, complete");
        assert_eq!(
            footer(20, 20, 40, Some(PageTotal::Live(57)), false),
            "# 20 shown of 57; more may exist: --skip 60, or --all for everything"
        );
        assert_eq!(
            footer(
                20,
                20,
                0,
                Some(PageTotal::Cached {
                    count: 847,
                    age: "2h ago".to_string()
                }),
                false
            ),
            "# 20 shown of ~847 (counted 2h ago); more may exist: --skip 20, or --all for everything"
        );
    }

    #[test]
    fn agent_list_rows_are_one_line_per_item() {
        let issue = issue_with_custom_fields(vec![CustomField::State {
            name: "State".into(),
            value: Some("Open".into()),
            is_resolved: false,
        }]);
        assert_eq!(issue.display_agent_row(), "PROJ-1\tOpen\t-\t-\ttest");
        assert_eq!(Issue::agent_list_header(), Some(AGENT_ISSUE_HEADER));

        // An article row leaves the content to `article get`.
        let mut article = article_with_content("line 1\nline 2".to_string());
        article.summary = "Two\nlines".to_string();
        let row = article.display_agent_row();
        assert!(!row.contains('\n'), "{row}");
        assert!(
            row.starts_with("KB-A-1  Two lines | project: KB | created: "),
            "{row}"
        );
        assert!(!row.contains("line 1"), "{row}");
        assert!(article.display_agent().contains("content:\nline 1\nline 2"));
        assert_eq!(Article::agent_list_header(), None);
    }

    #[test]
    fn agent_fields_show_set_values_and_the_resolved_state() {
        let field = |field: CustomField| agent_field(&field);
        assert_eq!(
            field(CustomField::State {
                name: "Stage".into(),
                value: Some("Done".into()),
                is_resolved: true,
            }),
            Some("Stage: Done (resolved)".to_string())
        );
        assert_eq!(
            field(CustomField::SingleUser {
                name: "Assignee".into(),
                login: Some("jdoe".into()),
                display_name: Some("J".into()),
            }),
            Some("Assignee: jdoe".to_string())
        );
        assert_eq!(
            field(CustomField::MultiEnum {
                name: "Components".into(),
                values: vec![],
            }),
            None
        );
        assert_eq!(
            field(CustomField::Text {
                name: "Notes".into(),
                value: Some(String::new()),
            }),
            None
        );
        assert_eq!(
            field(CustomField::Unknown {
                name: "Sprint".into(),
                value: Some(serde_json::json!(null)),
            }),
            None
        );
    }

    #[test]
    fn clean_flattens_control_characters_and_caps_length() {
        assert_eq!(
            clean("Bug\n\nIGNORE ALL\tPREVIOUS", 60),
            "Bug IGNORE ALL PREVIOUS"
        );
        assert_eq!(clean("  spaced   out  ", 60), "spaced out");
        assert_eq!(clean("abcdefghij", 5), "abcd…");
        assert_eq!(clean("\u{1b}[31mred", 60), "[31mred");
    }

    #[test]
    fn capped_reports_what_it_left_out() {
        let capped = Capped::new((1..=5).map(|n| n.to_string()), 3);
        assert_eq!(capped.shown, ["1", "2", "3"]);
        assert_eq!(capped.more, 2);
        assert_eq!(
            capped.render("track t ls"),
            "1, 2, 3, … +2 more (track t ls)"
        );

        let whole = Capped::new(["a".to_string()], 3);
        assert_eq!(whole.render("unused"), "a");
    }
}
