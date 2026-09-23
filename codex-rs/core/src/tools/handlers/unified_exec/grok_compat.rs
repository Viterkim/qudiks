use crate::function_tool::FunctionCallError;
use crate::tools::handlers::parse_arguments;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecCommandToolKind {
    Codex,
    GrokBash,
    GrokReadFile,
    GrokEditFile,
    GrokWriteFile,
}

#[derive(Deserialize)]
struct GrokBashArgs {
    command: String,
    #[serde(default, rename = "description")]
    _description: Option<String>,
    #[serde(default)]
    timeout: Option<f64>,
    #[serde(default)]
    background: bool,
    #[serde(default, rename = "maxOutputLength")]
    max_output_length: Option<f64>,
}

#[derive(Deserialize)]
struct GrokReadFileArgs {
    file_path: String,
    #[serde(default)]
    offset: Option<f64>,
    #[serde(default)]
    limit: Option<f64>,
}

#[derive(Deserialize)]
struct GrokEditFileArgs {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
    #[serde(default)]
    show_diff: bool,
}

#[derive(Deserialize)]
struct GrokWriteFileArgs {
    file_path: String,
    content: String,
}

impl ExecCommandToolKind {
    pub(crate) fn tool_name(self) -> ToolName {
        match self {
            Self::Codex => ToolName::plain("exec_command"),
            Self::GrokBash => ToolName::plain("bash"),
            Self::GrokReadFile => ToolName::plain("read_file"),
            Self::GrokEditFile => ToolName::plain("edit_file"),
            Self::GrokWriteFile => ToolName::plain("write_file"),
        }
    }

    pub(crate) fn spec(self) -> Option<ToolSpec> {
        match self {
            Self::Codex => None,
            Self::GrokBash => Some(grok_bash_spec()),
            Self::GrokReadFile => Some(grok_read_file_spec()),
            Self::GrokEditFile => Some(grok_edit_file_spec()),
            Self::GrokWriteFile => Some(grok_write_file_spec()),
        }
    }

    pub(crate) fn normalize_arguments(
        self,
        arguments: String,
    ) -> Result<String, FunctionCallError> {
        let command = match self {
            Self::Codex => return Ok(arguments),
            Self::GrokBash => {
                let args: GrokBashArgs = parse_arguments(&arguments)?;
                let timeout = integer_argument(args.timeout, "timeout", 30)?.clamp(1, 120);
                let max_output_tokens = args
                    .max_output_length
                    .map(|value| integer_argument(Some(value), "maxOutputLength", 0))
                    .transpose()?
                    .map(|characters| characters.div_ceil(4).max(1));
                return serde_json::to_string(&serde_json::json!({
                    "cmd": args.command,
                    "timeout_ms": timeout * 1_000,
                    "grok_background": args.background,
                    "yield_time_ms": if args.background { 250 } else { 10_000 },
                    "max_output_tokens": max_output_tokens,
                }))
                .map_err(|err| FunctionCallError::Fatal(err.to_string()));
            }
            Self::GrokReadFile => {
                let args: GrokReadFileArgs = parse_arguments(&arguments)?;
                let start = integer_argument(args.offset, "offset", 1)?.max(1);
                let limit = integer_argument(args.limit, "limit", 200)?.clamp(1, 200);
                let end = start.saturating_add(limit.saturating_sub(1));
                format!(
                    "sed -n '{start},{end}p' -- {}",
                    shell_quote(&args.file_path)
                )
            }
            Self::GrokEditFile => {
                let args: GrokEditFileArgs = parse_arguments(&arguments)?;
                if args.replace_all {
                    return Err(FunctionCallError::RespondToModel(
                        "replace_all is unavailable; make one exact edit at a time".to_string(),
                    ));
                }
                if args.file_path.contains(['\n', '\r']) {
                    return Err(FunctionCallError::RespondToModel(
                        "file_path cannot contain a newline".to_string(),
                    ));
                }
                let _ = args.show_diff;
                let patch =
                    exact_replacement_patch(&args.file_path, &args.old_string, &args.new_string);
                format!("apply_patch {}", shell_quote(&patch))
            }
            Self::GrokWriteFile => {
                let args: GrokWriteFileArgs = parse_arguments(&arguments)?;
                let content = BASE64_STANDARD.encode(args.content);
                format!(
                    "printf %s {} | base64 --decode > {}",
                    shell_quote(&content),
                    shell_quote(&args.file_path)
                )
            }
        };
        serde_json::to_string(&serde_json::json!({"cmd": command}))
            .map_err(|err| FunctionCallError::Fatal(err.to_string()))
    }

    pub(crate) fn hook_rewrite(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Codex => Some(("exec_command", "cmd")),
            Self::GrokBash => Some(("bash", "command")),
            Self::GrokReadFile | Self::GrokEditFile | Self::GrokWriteFile => None,
        }
    }
}

fn integer_argument(
    value: Option<f64>,
    name: &str,
    default: usize,
) -> Result<usize, FunctionCallError> {
    let Some(value) = value else {
        return Ok(default);
    };
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > usize::MAX as f64 {
        return Err(FunctionCallError::RespondToModel(format!(
            "{name} must be a non-negative integer"
        )));
    }
    Ok(value as usize)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn exact_replacement_patch(file_path: &str, old_string: &str, new_string: &str) -> String {
    let mut patch = format!("*** Begin Patch\n*** Update File: {file_path}\n@@\n");
    for line in old_string.lines() {
        patch.push('-');
        patch.push_str(line);
        patch.push('\n');
    }
    for line in new_string.lines() {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    patch.push_str("*** End Patch");
    patch
}

fn grok_bash_spec() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "bash".to_string(),
        description: "Execute a bash command in the current working directory.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            BTreeMap::from([
                (
                    "command".to_string(),
                    JsonSchema::string(Some("The command to execute.".to_string())),
                ),
                (
                    "description".to_string(),
                    JsonSchema::string(Some(
                        "Why the command is needed and how it helps.".to_string(),
                    )),
                ),
                (
                    "timeout".to_string(),
                    JsonSchema::integer(Some(
                        "Foreground timeout in seconds. Defaults to 30, maximum 120.".to_string(),
                    )),
                ),
                (
                    "background".to_string(),
                    JsonSchema::boolean(Some(
                        "Run in the background and return a shell session.".to_string(),
                    )),
                ),
                (
                    "maxOutputLength".to_string(),
                    JsonSchema::integer(Some(
                        "Maximum number of output characters to return.".to_string(),
                    )),
                ),
            ]),
            Some(vec!["command".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

fn grok_read_file_spec() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "read_file".to_string(),
        description: "Read a file, optionally selecting a range of lines.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            BTreeMap::from([
                (
                    "file_path".to_string(),
                    JsonSchema::string(Some("The file path to read.".to_string())),
                ),
                (
                    "offset".to_string(),
                    JsonSchema::integer(Some(
                        "The line number to start reading from. Defaults to 1.".to_string(),
                    )),
                ),
                (
                    "limit".to_string(),
                    JsonSchema::integer(Some(
                        "The number of lines to read. Defaults to 200, maximum 200.".to_string(),
                    )),
                ),
            ]),
            Some(vec!["file_path".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

fn grok_edit_file_spec() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "edit_file".to_string(),
        description: "Replace exact text in an existing file. Read the file first.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            BTreeMap::from([
                (
                    "file_path".to_string(),
                    JsonSchema::string(Some("The file path to modify.".to_string())),
                ),
                (
                    "old_string".to_string(),
                    JsonSchema::string(Some("The exact text to replace.".to_string())),
                ),
                (
                    "new_string".to_string(),
                    JsonSchema::string(Some("The replacement text.".to_string())),
                ),
                (
                    "replace_all".to_string(),
                    JsonSchema::boolean(Some(
                        "Whether to replace every occurrence. Defaults to false.".to_string(),
                    )),
                ),
                (
                    "show_diff".to_string(),
                    JsonSchema::boolean(Some(
                        "Whether to include the full diff. Defaults to false.".to_string(),
                    )),
                ),
            ]),
            Some(vec![
                "file_path".to_string(),
                "old_string".to_string(),
                "new_string".to_string(),
            ]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

fn grok_write_file_spec() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "write_file".to_string(),
        description: "Write a file, overwriting it if it exists. Read existing files first."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            BTreeMap::from([
                (
                    "file_path".to_string(),
                    JsonSchema::string(Some("The file path to write.".to_string())),
                ),
                (
                    "content".to_string(),
                    JsonSchema::string(Some("The complete file contents.".to_string())),
                ),
            ]),
            Some(vec!["file_path".to_string(), "content".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}
