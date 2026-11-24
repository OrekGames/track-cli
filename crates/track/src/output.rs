use crate::cli::OutputFormat;
use colored::Colorize;
use serde::Serialize;
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

/// `--select` paths, set once before dispatch. Every JSON document a command
/// prints goes through [`output_json`], so projecting there covers them all.
static SELECT_PATHS: OnceLock<Vec<Vec<String>>> = OnceLock::new();

/// Project every JSON document onto these paths.
pub fn set_select_paths(paths: Vec<Vec<String>>) {
    let _ = SELECT_PATHS.set(paths);
}

/// Render every JSON document as compact JSON Lines instead of pretty JSON.
pub fn set_jsonl_mode(enabled: bool) {
    JSONL_MODE.store(enabled, Ordering::Relaxed);
}

/// Print `value` as pretty JSON, or as JSONL under `-o jsonl`.
pub fn output_json<T: Serialize + ?Sized>(value: &T) -> anyhow::Result<()> {
    let jsonl = JSONL_MODE.load(Ordering::Relaxed);
    match SELECT_PATHS.get() {
        // Without `--select` the value is written as is, so key order and
        // formatting stay exactly what they have always been.
        None => print_json(value, jsonl),
        Some(paths) => print_json(
            &select_from(serde_json::to_value(value)?, paths, true)?,
            jsonl,
        ),
    }
}

/// One line of a JSON Lines stream that a command writes itself, projected
/// like everything else when `--select` is set.
pub fn jsonl_line<T: Serialize + ?Sized>(value: &T) -> anyhow::Result<String> {
    match SELECT_PATHS.get() {
        None => Ok(serde_json::to_string(value)?),
        // One line of many: a line the paths do not reach (a failure
        // record, say) is not a reason to stop the stream.
        Some(paths) => Ok(serde_json::to_string(&select_from(
            serde_json::to_value(value)?,
            paths,
            false,
        )?)?),
    }
}

/// Project `value` onto `--select` paths.
///
/// A path segment named `fields` reaches a flat view of an issue's custom
/// fields (`fields.Priority`), computed here so the default output does not
/// carry it. A selection that matches nothing is an error: an empty object
/// would read as "this issue has no such data".
fn select_from(mut value: Value, paths: &[Vec<String>], must_match: bool) -> anyhow::Result<Value> {
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
    if must_match && matched_nothing {
        let shown: Vec<String> = paths.iter().map(|path| path.join(".")).collect();
        return Err(crate::errors::CliError::new(
            "select_no_match",
            format!(
                "--select matched nothing: no '{}' in this command's JSON output",
                shown.join("', '")
            ),
        )
        .with_hint("Run the command once without --select to see the field names.")
        .into());
    }
    Ok(projected)
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
/// `("Priority", "Major")`. Fields without a value are left out.
fn flat_field(field: &Value) -> Option<(String, Value)> {
    let (variant, body) = field.as_object()?.iter().next()?;
    let name = body.get("name")?.as_str()?.to_string();
    let value = match variant.as_str() {
        "MultiEnum" => body.get("values"),
        "SingleUser" => body.get("login").filter(|login| !login.is_null()),
        _ => body.get("value"),
    }?;
    (!value.is_null()).then(|| (name, value.clone()))
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
            for item in items {
                println!("{}", item.display_agent());
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
        .and_then(|f| match f {
            CustomField::SingleEnum { value, .. } => value.clone(),
            CustomField::State { value, .. } => value.clone(),
            CustomField::SingleUser { login, .. } => login.clone(),
            CustomField::Text { value, .. } => value.clone(),
            CustomField::MultiEnum { values, .. } => Some(values.join(", ")),
            CustomField::Unknown { value, .. } => value.as_ref().map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }),
        })
}

/// Report a failure on stderr: what went wrong, its stable code, and the
/// next step when one is known.
pub fn output_error(err: &anyhow::Error, format: crate::errors::ErrorFormat) {
    let report = crate::errors::describe(err);
    match format {
        crate::errors::ErrorFormat::Json => {
            eprintln!("{}", crate::errors::render(&report, format));
        }
        crate::errors::ErrorFormat::Text => {
            eprintln!("{}: {}", "Error".red().bold(), report.message);
            eprintln!("{}: {}", "Code".dimmed(), report.code);
            if let Some(hint) = &report.hint {
                eprintln!("{}: {}", "Hint".yellow(), hint);
            }
        }
    }
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

/// Print a pagination hint to stderr when results fill the page limit.
/// Only prints in text mode; no TTY gate (goes to stderr, won't pollute piped output).
pub fn output_page_hint(
    result_count: usize,
    limit: usize,
    skip: usize,
    cached_total: Option<(u64, &str)>,
    format: OutputFormat,
) {
    // JSON gets the hint too: a full page of JSON looks exactly like a
    // complete result. Agent mode states completeness on stdout instead
    // (see [`agent_page_footer`]).
    let _ = format;
    if is_agent() {
        return;
    }
    if result_count == 0 || result_count < limit {
        return; // partial page or empty — we have all results
    }
    // Full page — there may be more
    let next_skip = skip + result_count;

    let total_part = match cached_total {
        Some((total, "live")) => format!(" ({} of {} total)", next_skip, total),
        Some((total, age)) => format!(" (~{} total, {})", total, age),
        None => String::new(),
    };

    if skip == 0 {
        // First page: suggest both --all and --skip
        eprintln!(
            "  {} {} results shown{}  ·  use {} or {} for next page",
            "┄┄".dimmed(),
            result_count,
            total_part.dimmed(),
            "--all".cyan(),
            format!("--skip {}", next_skip).cyan(),
        );
    } else {
        // Already paginating with --skip: only suggest --skip (--all conflicts with --skip in clap)
        eprintln!(
            "  {} {} results shown{}  ·  use {} for next page",
            "┄┄".dimmed(),
            result_count,
            total_part.dimmed(),
            format!("--skip {}", next_skip).cyan(),
        );
    }
}

pub trait Displayable {
    fn display(&self) -> String;

    /// The `-o agent` rendering: as few bytes as carry the whole record.
    /// Defaults to the terminal rendering, which is uncoloured in that mode.
    fn display_agent(&self) -> String {
        self.display()
    }
}

/// `Name: value` for a custom field that has a value.
fn agent_field(field: &CustomField) -> Option<String> {
    let (name, value) = match field {
        CustomField::SingleEnum { name, value } | CustomField::Text { name, value } => {
            (name, value.clone()?)
        }
        CustomField::State {
            name,
            value,
            is_resolved,
        } => {
            let value = value.clone()?;
            (
                name,
                if *is_resolved {
                    format!("{value} (resolved)")
                } else {
                    value
                },
            )
        }
        CustomField::SingleUser { name, login, .. } => (name, login.clone()?),
        CustomField::MultiEnum { name, values } => {
            if values.is_empty() {
                return None;
            }
            (name, values.join(", "))
        }
        CustomField::Unknown { name, value } => (
            name,
            match value.as_ref()? {
                Value::String(text) => text.clone(),
                Value::Null => return None,
                other => other.to_string(),
            },
        ),
    };
    (!value.is_empty()).then(|| format!("{name}: {value}"))
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

/// The last line of an agent-mode list: whether it is the whole result and,
/// if not, how to get the rest. Printed to stdout so it cannot be missed.
pub fn agent_page_footer(
    result_count: usize,
    limit: usize,
    skip: usize,
    total: Option<(u64, &str)>,
    fetched_all: bool,
) -> String {
    if fetched_all || result_count < limit {
        return format!("# {} shown, complete", result_count);
    }
    let next_skip = skip + result_count;
    let of_total = match total {
        Some((total, "live")) => format!(" of {total}"),
        Some((total, age)) => format!(" of ~{total} (counted {age})"),
        None => String::new(),
    };
    format!(
        "# {result_count} shown{of_total}; more may exist: --skip {next_skip}, or --all for everything"
    )
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
        let mut meta = vec![format!(
            "project: {}",
            self.project
                .short_name
                .as_deref()
                .unwrap_or(&self.project.id)
        )];
        if let Some(parent) = &self.parent_article {
            meta.push(format!(
                "parent: {}",
                parent.id_readable.as_deref().unwrap_or(&parent.id)
            ));
        }
        if self.has_children {
            meta.push("has children".to_string());
        }
        if !self.tags.is_empty() {
            let names: Vec<&str> = self.tags.iter().map(|t| t.name.as_str()).collect();
            meta.push(format!("tags: {}", names.join(", ")));
        }
        meta.push(format!("created: {}", self.created.format("%Y-%m-%d")));
        meta.push(format!("updated: {}", self.updated.format("%Y-%m-%d")));
        output.push('\n');
        output.push_str(&meta.join(" | "));
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

    #[test]
    fn page_hint_suppressed_on_partial_page() {
        // limit=20, got 7 — no hint expected (just verify it returns without panic)
        output_page_hint(7, 20, 0, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_suppressed_in_json_mode() {
        output_page_hint(20, 20, 0, None, OutputFormat::Json);
    }

    #[test]
    fn page_hint_suppressed_on_zero_results() {
        // Edge case: 0 results with limit=0 should not fire
        output_page_hint(0, 0, 0, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_fires_on_full_page() {
        // limit=20, got 20 — hint should fire (we can't capture stderr easily in unit test,
        // so just verify it doesn't panic)
        output_page_hint(20, 20, 0, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_with_skip() {
        // When skip > 0, should still work but only suggest --skip
        output_page_hint(20, 20, 40, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_with_cached_total() {
        // Verify it doesn't panic with a cached total
        output_page_hint(20, 20, 0, Some((847, "2h ago")), OutputFormat::Text);
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
                "Stage": "Done", "Assignee": "jdoe", "Components": ["A", "B"],
                "Sprint": [{"id": 12}]
            })
        );
        assert_eq!(doc["issues"][0]["fields"], serde_json::json!({}));
    }

    #[test]
    fn select_must_match_only_when_asked() {
        let paths = parse_select_paths("nope");
        let value = serde_json::json!({"id": 1});
        assert!(select_from(value.clone(), &paths, true).is_err());
        assert_eq!(
            select_from(value, &paths, false).unwrap(),
            serde_json::json!({})
        );
        // An empty list selects to an empty list: nothing was missed.
        assert_eq!(
            select_from(serde_json::json!([]), &paths, true).unwrap(),
            serde_json::json!([])
        );
    }

    #[test]
    fn agent_footer_says_complete_or_how_to_continue() {
        assert_eq!(
            agent_page_footer(7, 20, 0, None, false),
            "# 7 shown, complete"
        );
        assert_eq!(
            agent_page_footer(0, 20, 0, None, false),
            "# 0 shown, complete"
        );
        assert_eq!(
            agent_page_footer(500, 20, 0, None, true),
            "# 500 shown, complete"
        );
        assert_eq!(
            agent_page_footer(20, 20, 40, Some((57, "live")), false),
            "# 20 shown of 57; more may exist: --skip 60, or --all for everything"
        );
        assert_eq!(
            agent_page_footer(20, 20, 0, Some((847, "2h ago")), false),
            "# 20 shown of ~847 (counted 2h ago); more may exist: --skip 20, or --all for everything"
        );
    }
}
